//use proc_macro::{Delimiter, Punct, TokenStream, TokenTree};

pub mod api;
pub mod config;
pub mod database;
pub mod util;
//man that was a waste of time

/*
#[proc_macro_derive(Params)]
pub fn add_struct_params(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut hit_colon = false;
    let mut fields: Vec<(String, String)> = vec![];
    for token in item.into_iter() {
        match token {
            TokenTree::Group(group) => {
                if group.delimiter() == Delimiter::Brace {
                    for item in group.stream() {
                        println!("{:?}", item);
                        match item {
                            TokenTree::Group(_) => {}
                            TokenTree::Ident(indent) => {
                                if !hit_colon {
                                    if !fields
                                        .last()
                                        .unwrap_or(&("".to_string(), "nonempty string".to_string()))
                                        .1.is_empty()
                                    {
                                        fields.push((indent.to_string(), "".parse().unwrap()));
                                    } else {
                                        fields.last_mut().unwrap().0 = indent.to_string();
                                    }
                                } else {
                                    fields.last_mut().unwrap().1 += indent.to_string().as_str();
                                }
                            }
                            TokenTree::Punct(punct) => match punct.as_char() {
                                ':' => {
                                    hit_colon = true;
                                }
                                ',' => {
                                    hit_colon = false;
                                }
                                _ => {
                                    fields.last_mut().unwrap().1 += punct.to_string().as_str();
                                }
                            },
                            TokenTree::Literal(_) => {}
                        }
                    }
                }
            }
            TokenTree::Ident(_) => {}
            TokenTree::Punct(_) => {}
            TokenTree::Literal(_) => {}
        }
    }
    println!("{:#?}", fields);
    let mut out = String::new();
    /*
    out += format![
        "fn params(&self) -> &[&dyn ::rusqlite::ToSql] {{[{}]}}",
        fields
            .iter()
            .map(|(field_name, field_type)| {
                format!["&(self.{}) as &dyn ::rusqlite::ToSql", field_name]
            })
            .collect::<Vec<_>>()
            .join(", ")
    ]
    .as_str();
     */

    out += format![
        "fn columns() -> Vec<(&'static str, &'static str)> {{{}}}",
        fields.iter().map(|(field_name, field_type)|)
    ];

    println!("{:#?}", out);
    out.parse().unwrap()
}
 */
