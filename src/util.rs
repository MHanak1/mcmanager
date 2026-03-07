use std::sync::LazyLock;

use any_ascii::any_ascii;
use color_eyre::Result;
use color_eyre::eyre::bail;
use regex::Regex;

pub static RE_SLUG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z1-9-]+$").expect("Invalid Regex"));

pub static RE_USERNAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-z1-9-]+$").expect("Invalid Regex"));

pub fn sanitise_slug(input: &str) -> String {
    let input = any_ascii(input);
    let input = input.to_lowercase();

    let mut slug = String::with_capacity(input.len());

    let mut was_dash = false;
    for char in input.chars() {
        match char {
            'a'..='z' | '0'..='9' => {
                was_dash = false;
                slug.push(char);
            }
            _ => {
                if char.is_whitespace() && !was_dash {
                    slug.push('-');
                    was_dash = true;
                }
            }
        }
    }

    slug
}

pub fn validate_slug(slug: &str) -> Result<()> {
    if !RE_SLUG.is_match(slug) {
        bail!("Slug contains invalid characters");
    }
    Ok(())
}

// This is separate from the #[validate] on PartialUser
pub fn validate_username(username: &str) -> Result<()> {
    if username.len() < 3 {
        bail!("Username must be at least 3 characters long");
    }

    if username.len() > 32 {
        bail!("The password must be at most 64 characters long");
    }

    if !RE_USERNAME.is_match(username) {
        bail!("Username contains invalid characters");
    }

    Ok(())
}

// This is separate from the #[validate] on PartialUser
pub fn validate_password(password: &str) -> Result<()> {
    if password.len() < 8 {
        bail!("The password must be at least 8 characters long");
    }

    if password.len() > 1024 {
        bail!("1024 characters is plenty long, you don't need more.");
    }

    Ok(())
}
