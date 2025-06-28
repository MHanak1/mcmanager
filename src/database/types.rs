use crate::database::objects::{DbObject, Group, User};
pub(crate) use crate::database::{DatabaseType, ValueType};
use crate::util;
use crate::util::base64::base64_encode;
use anyhow::Result;
use rand::TryRngCore;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::{Column as SqlxColumn, ColumnIndex, Database, Decode, Encode, Row, Type};
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use test_log::test;

pub(crate) const ID_MAX_VALUE: i64 = 281_474_976_710_655;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub data_type: ValueType,
    pub modifiers: Vec<Modifier>,
    pub nullable: bool,
    pub hidden: bool,
}

impl Column {
    pub fn new(name: &str, data_type: ValueType) -> Self {
        Self {
            name: name.to_string(),
            data_type,
            modifiers: Vec::new(),
            nullable: true,
            hidden: false,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn descriptor(&self) -> String {
        let mut descriptor = self.data_type.descriptor();
        for modifier in &self.modifiers {
            descriptor = modifier.apply_to(&descriptor);
        }
        descriptor
    }

    pub fn with_modifier(self, modifier: Modifier) -> Self {
        let mut new = self;
        new.modifiers.push(modifier);
        new
    }

    pub fn primary_key(self) -> Self {
        self.with_modifier(Modifier::PrimaryKey)
    }

    pub fn not_null(self) -> Self {
        let mut new = self;
        new.nullable = false;
        new.with_modifier(Modifier::NotNull)
    }

    pub fn unique(self) -> Self {
        self.with_modifier(Modifier::Unique)
    }

    pub fn references(self, value: &str) -> Self {
        self.with_modifier(Modifier::References(value.to_string()))
    }

    pub fn default(self, value: &str) -> Self {
        self.with_modifier(Modifier::Default(value.to_string()))
    }

    pub fn hidden(self) -> Self {
        let mut new = self;
        new.hidden = true;
        new
    }
}

impl<T: Row> ColumnIndex<T> for Column {
    fn index(&self, container: &T) -> std::result::Result<usize, sqlx::Error> {
        match container.columns().iter().find_map(|column| {
            if column.name() == self.name {
                Some(column)
            } else {
                None
            }
        }) {
            Some(column) => Ok(column.ordinal()),
            None => Err(sqlx::Error::ColumnNotFound(self.name.clone())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modifier {
    PrimaryKey,
    NotNull,
    Unique,
    References(String),
    Default(String),
}

impl Modifier {
    pub fn descriptor(&self) -> String {
        match self {
            Modifier::PrimaryKey => "PRIMARY KEY".to_string(),
            Modifier::NotNull => "NOT NULL".to_string(),
            Modifier::Unique => "UNIQUE".to_string(),
            Modifier::References(s) => format!("REFERENCES {s}"),
            Modifier::Default(s) => format!("DEFAULT {s}"),
        }
    }

    pub fn apply_to(&self, value: &str) -> String {
        format!("{} {}", value, self.descriptor())
    }
}

/// Access level
///
/// [`Access::All`]: the access will always pass
/// [`Access::User`]: every active user has access
/// [`Access::Owner(column_name: String)`]: only the owner has access. to use this on a [`DbObject`], the parameter is the name of the column accessing user's id will be matched with
/// [`Access::PrivilegedUser`]: every user with `privileged = true` has access
/// [`Access::None`]: access is always denied
pub enum Access {
    All,
    User,
    Owner(&'static str),
    PrivilegedUser,
    None,

    And(Box<Access>, Box<Access>),
    Or(Box<Access>, Box<Access>),
}

impl Access {
    pub fn or(self, other: Access) -> Self {
        Access::Or(Box::new(self), Box::new(other))
    }
    pub fn and(self, other: Access) -> Self {
        Access::And(Box::new(self), Box::new(other))
    }

    /// whether a user can passes the access check.
    ///
    /// # Panics
    ///
    /// if the access level is [`Access::Owner`], the `object` must me [`Some`].
    #[allow(clippy::expect_fun_call)]
    pub fn can_access<'a, T: DbObject + ?Sized>(
        &self,
        object: Option<&T>,
        user: &User,
        group: &Group,
    ) -> bool {
        match self {
            Access::All => true,
            Access::And(left, right) => {
                right.can_access(object, user, group) && left.can_access(object, user, group)
            }
            Access::Or(left, right) => {
                right.can_access(object, user, group) || left.can_access(object, user, group)
            }
            _ => {
                //all the following restrict the user to be enabled
                if user.enabled {
                    match self {
                        Access::User => true,
                        //Access::Owner() => object.expect("owner access used with object being None").params()[] == user.id.to_sql().unwrap(),
                        Access::Owner(_) => {
                            let object = object.expect("Access::User must provide an object");
                            object
                                .owner_id()
                                .expect("object does not implement owner_id()")
                                == user.id
                        }
                        Access::PrivilegedUser => group.is_privileged,
                        Access::None => false,
                        Access::All | Access::And(..) | Access::Or(..) => {
                            unreachable!();
                        }
                    }
                } else {
                    false
                }
            }
        }
    }

    pub fn access_filter<'a, T: DbObject + ?Sized>(&self, user: &User, group: &Group) -> String {
        match self {
            Access::All => "1".to_string(),
            Access::And(left, right) => {
                format!(
                    "({} AND {})",
                    left.access_filter::<T>(user, group),
                    right.access_filter::<T>(user, group)
                )
            }
            Access::Or(left, right) => {
                format!(
                    "({} OR {})",
                    left.access_filter::<T>(user, group),
                    right.access_filter::<T>(user, group)
                )
            }
            _ => {
                //all the following restrict the user to be enabled
                if user.enabled {
                    match self {
                        Access::User => "1".to_string(),
                        Access::Owner(owner_column_name) => {
                            format!("{}={}", owner_column_name, user.id.as_i64())
                        }
                        Access::PrivilegedUser => {
                            if group.is_privileged { "1" } else { "0" }.to_string()
                        }
                        Access::None => "0".to_string(),
                        Access::All | Access::And(..) | Access::Or(..) => {
                            unreachable!();
                        }
                    }
                } else {
                    "0".to_string()
                }
            }
        }
    }
}

/// Id holds a 48-bit identifier, which can be accessed in a form of an `i64` or as an URL-safe base 64 encoded 8 character `string`
///
/// It should be used in the numeric form in in the low level in the backend (eg. database fields), and in the string form everywhere else (like `JSON` fields).
///
/// `Default::default()` generates a random Id
#[derive(Clone, Copy, PartialEq, Eq, Type)]
#[sqlx(transparent)]
pub struct Id(i64);

impl Id {
    pub fn from_i64(value: i64) -> Result<Self> {
        if value > ID_MAX_VALUE {
            return Err(anyhow::anyhow!("id is out of the 48 bit range"));
        }
        Ok(Self(value))
    }
    #[deprecated(note = "please use `from_u64` instead")]
    pub fn from_u64(value: u64) -> Result<Self> {
        Self::from_i64(value as i64)
    }

    pub fn from_string(s: &str) -> Result<Self> {
        if s.len() != 8 {
            return Err(anyhow::anyhow!("The provided id must be 8 characters long"));
        }
        let id_slice = util::base64::base64_decode(s);
        if id_slice.is_err() {
            return Err(anyhow::anyhow!("Failed to parse id"));
        }

        let id_slice = id_slice.expect("failed to optain the id's slice");

        let mut id = 0i64;
        for i in id_slice {
            id <<= 8;
            id |= i64::from(i);
        }

        Ok(Self(id))
    }

    pub fn new_random() -> Self {
        let val = rand::random_range(0..ID_MAX_VALUE);
        Self::from_i64(val).expect("failed to create a new id")
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }

    pub fn to_sql_string(&self) -> String {
        self.as_i64().to_string()
    }
}

/*
impl<'r, DB: Database> Decode<'r, DB> for Id
where
    &'r str: Decode<'r, DB>
{
    fn decode(
        value: <DB as Database>::ValueRef<'r>,
    ) -> Result<Id, Box<dyn Error + 'static + Send + Sync>> {
        let value = <&i64 as Decode<DB>>::decode(value)?;
        Ok(value.parse()?)
    }
}
 */

impl FromStr for Id {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::from_string(s)
    }
}
impl Serialize for Id {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match Self::from_string(s.as_str()) {
            Ok(id) => Ok(id),
            Err(err) => Err(Error::custom(err.to_string())),
        }
    }
}

impl Hash for Id {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash(state);
    }
}

impl From<Id> for i64 {
    fn from(value: Id) -> Self {
        value.0
    }
}

impl From<Id> for String {
    fn from(value: Id) -> Self {
        value.to_string()
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            &base64_encode(&self.as_i64().to_be_bytes().as_slice()[2..8])
        )
    }
}

impl Debug for Id {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new_random()
    }
}

#[test]
fn id() {
    use pretty_assertions::assert_eq;

    assert_eq!(
        Id::from_i64(0).expect("failed to create id from i64 (0)"),
        Id { id: 0 }
    );
    assert!(Id::from_i64(ID_MAX_VALUE + 1).is_err());

    assert_eq!(
        Id::from_i64(ID_MAX_VALUE).expect("failed to create id from i64 (ID_MAX_VALUE)"),
        Id::from_string("________").expect("failed to create id from string (________)")
    ); // "________" is the max possible value
    assert_eq!(
        Id::from_string("________")
            .expect("failed to create id from string (________)")
            .to_string(),
        "________".to_string()
    );

    //an id picked at random
    assert_eq!(
        Id::from_i64(236540241151257).expect("failed to create id from i64 (236540241151257)"),
        Id::from_string("1yHRDIUZ").expect("failed to create id from string (1yHRDIUZ)")
    );
    assert_eq!(
        Id::from_i64(236540241151257)
            .expect("failed to create id from i64 (236540241151257)")
            .to_string(),
        "1yHRDIUZ"
    );

    assert_eq!(Id::new_random().to_string().len(), 8);
}

/*
#[derive(Debug, PartialEq, Eq, Clone, Type, Encode, Decode)]
#[sqlx(transparent)]
pub struct Token ([u64; 4]);

/// A base64 encoded auth token. By default it has the length of 4 (giving 32 bytes)
impl Token {
    pub fn new() -> Self {
        let mut rng = rand::rngs::OsRng;

        /*
        let token = (0..4)
            .map(|_| {
                base64_encode(
                    &rng.try_next_u64()
                        .expect("failed to url_encode the value")
                        .to_be_bytes(),
                )
            })
            .collect::<String>();
         */
        let token = (0..4).map(|_| rng.next_u64()).collect::<Vec<_>>().try_into().unwrap();


        Self { token }
    }

    pub fn from_string_ckecked(string: String) -> Result<Self> {
        //check if is decodable
        let vals = base64_decode(string.as_str())?;
        let token = (0..4).map(|i| (vals[i * 4] as u64) << 48 | (vals[i * 4 + 1] as u64) << 32 | (vals[i * 4 + 2] as u64) << 16 | (vals[i * 4 + 3] as u64)).collect::<Vec<_>>().try_into().unwrap();

        Ok(Self {token})
    }
}

impl Default for Token {
    fn default() -> Self {
        Self::new(4)
    }
}
impl Display for Token {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.token)
    }
}

impl From<Token> for String {
    fn from(value: Token) -> Self {
        value.to_string()
    }
}

impl From<String> for Token {
    fn from(value: String) -> Self {
        Self { token: value }
    }
}

impl FromStr for Token {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::from_string_ckecked(s.to_string())
    }
}


impl Serialize for Token {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Token {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match Self::from_string_ckecked(s) {
            Ok(id) => Ok(id),
            Err(err) => Err(Error::custom(err.to_string())),
        }
    }
}
*/
impl ValueType {
    pub fn descriptor(&self) -> String {
        match self {
            ValueType::Integer(signed, ..) => {
                if *signed {
                    "INTEGER".to_string()
                } else {
                    "UNSIGNED INTEGER".to_string()
                }
            }
            ValueType::Float => "FLOAT".to_string(),
            ValueType::Text => "TEXT".to_string(),
            ValueType::Boolean => "BOOLEAN".to_string(),
            ValueType::Blob => "BLOB".to_string(),
            ValueType::Id => "UNSIGNED BIGINT".to_string(),
            ValueType::Token => "TEXT".to_string(),
            ValueType::Datetime => "DATETIME".to_string(),
        }
    }
}
