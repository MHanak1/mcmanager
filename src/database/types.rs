use crate::util;
use crate::util::base64::{base64_decode, base64_encode};
use anyhow::Result;
use rand::TryRngCore;
use rusqlite::ToSql;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Debug, Display, Formatter};

pub(crate) const ID_MAX_VALUE: i64 = 281_474_976_710_655;

/// Id holds a 48-bit identifier, which can be accessed in a form of an `i64` or as an URL-safe base 64 encoded 8 character `string`
///
/// It should be used in the numeric form in in the low level in the backend (eg. database fields), and in the string form everywhere else (like `JSON` fields).
///
/// `Default::default()` generates a random Id
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Id {
    id: i64,
}

impl Id {
    pub fn from_i64(value: i64) -> Result<Self> {
        if value > ID_MAX_VALUE {
            return Err(anyhow::anyhow!("id is out of the 48 bit range"));
        }
        Ok(Self { id: value })
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

        let id_slice = id_slice.unwrap();

        let mut id = 0i64;
        for i in id_slice {
            id <<= 8;
            id |= i64::from(i);
        }

        Ok(Self { id })
    }

    pub fn new_random() -> Self {
        let val = rand::random_range(0..ID_MAX_VALUE);
        Self::from_i64(val).unwrap()
    }

    pub fn as_i64(self) -> i64 {
        self.id
    }
}

impl FromSql for Id {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Self::from_i64(value.as_i64()?).map_or_else(|_| Err(FromSqlError::InvalidType), Ok)
    }
}

impl ToSql for Id {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_i64())) //we need this tomfoolery to convert u64 into i64 because rusqlite doesn't allow u64
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

impl From<Id> for i64 {
    fn from(value: Id) -> Self {
        value.id
    }
}

impl From<Id> for String {
    fn from(value: Id) -> Self {
        value.to_string()
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            &base64_encode(&self.as_i64().to_be_bytes().as_slice()[2..8])
        )
    }
}

impl Debug for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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
    assert_eq!(Id::from_i64(0).unwrap(), Id { id: 0 });
    assert!(Id::from_i64(ID_MAX_VALUE + 1).is_err());

    assert_eq!(
        Id::from_i64(ID_MAX_VALUE).unwrap(),
        Id::from_string("________").unwrap()
    ); // "________" is the max possible value
    assert_eq!(
        Id::from_string("________").unwrap().to_string(),
        "________".to_string()
    );

    //an id picked at random
    assert_eq!(
        Id::from_i64(236540241151257).unwrap(),
        Id::from_string("1yHRDIUZ").unwrap()
    );
    assert_eq!(
        Id::from_i64(236540241151257).unwrap().to_string(),
        "1yHRDIUZ"
    );

    assert_eq!(Id::new_random().to_string().len(), 8);
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Token {
    token: String,
}

/// A base64 encoded auth token. By default it has the length of 4 (giving 32 bytes)
impl Token {
    pub fn new(size: usize) -> Self {
        let mut rng = rand::rngs::OsRng;

        let token = (0..size)
            .map(|_| base64_encode(&rng.try_next_u64().unwrap().to_be_bytes()))
            .collect::<String>();
        Self { token }
    }

    pub fn from_string_ckecked(string: String) -> Result<Self> {
        //check if is decodable
        match base64_decode(string.as_str()) {
            Ok(_) => {
                Ok(Self {token: string })
            }
            Err((err, _)) => {
                Err(anyhow::anyhow!(err.to_string()))
            }
        }
    }
}

impl Default for Token {
    fn default() -> Self {
        Self::new(4)
    }
}
impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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
impl FromSql for Token {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(Self {
            token: value.as_str()?.to_string(),
        })
    }
}

impl ToSql for Token {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.token.as_str()))
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
