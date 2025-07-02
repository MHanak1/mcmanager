pub mod dirs {
    use std::env;
    use std::fs::create_dir_all;
    use std::path::PathBuf;

    pub fn base_dir() -> PathBuf {
        env::current_exe()
            .expect("Can't get current executable path")
            .parent()
            .expect("Can't get parent directory of executable")
            .to_owned()
    }
    pub fn data_dir() -> PathBuf {
        base_dir().join("data")
    }
    pub fn icons_dir() -> PathBuf {
        data_dir().join("icons")
    }
    pub fn user_avatars_dir() -> PathBuf {icons_dir().join("users")}
    pub fn mod_icons_dir() -> PathBuf {icons_dir().join("mods")}
    pub fn world_icons_dir() -> PathBuf {icons_dir().join("worlds")}
    pub fn velocity_dir() -> PathBuf {
        data_dir().join("velocity")
    }
    pub fn versions_dir() -> PathBuf {
        data_dir().join("versions")
    }
    pub fn worlds_dir() -> PathBuf {
        data_dir().join("worlds")
    }

    pub fn init_dirs() -> anyhow::Result<()> {
        create_dir_all(data_dir())?;

        create_dir_all(user_avatars_dir())?;
        create_dir_all(mod_icons_dir())?;
        create_dir_all(world_icons_dir())?;

        create_dir_all(velocity_dir())?;
        create_dir_all(versions_dir())?;
        create_dir_all(worlds_dir())?;

        Ok(())
    }
}

#[macro_export]
macro_rules! execute_on_enum {
    ($enum_:expr; ($variant:path) |$value:ident| $block:block) => {
        match $enum_ {
            $variant ($value) => $block,
            _ => unreachable!()
        }
    };
    ($enum_:expr; ($variant1:path, $variant2:path) |$value:ident| $block:block) => {
        match $enum_ {
            $variant1 ($value) => $block,
            $variant2 ($value) => $block,
        }
    };

    ($enum_:expr; ($variant1:path, $variant2:path, $variant3:path) |$value:ident| $block:block) => {
        match $enum_ {
            $variant1 ($value) => $block,
            $variant2 ($value) => $block,
            $variant3 ($value) => $block,
        }
    };

    ($enum_:expr; ($variant1:path, $variant2:path, $variant3:path, $variant4:path) |$value:ident| $block:block) => {
        match $enum_ {
            $variant1 ($value) => $block,
            $variant2 ($value) => $block,
            $variant3 ($value) => $block,
            $variant4 ($value) => $block,
        }
    };

    ($enum_:expr; ($variant1:path, $variant2:path, $variant3:path, $variant4:path, $variant5:path) |$value:ident| $block:block) => {
        match $enum_ {
            $variant1 ($value) => $block,
            $variant2 ($value) => $block,
            $variant3 ($value) => $block,
            $variant4 ($value) => $block,
            $variant5 ($value) => $block,
        }
    };

    ($enum_:expr; ($variant1:path, $variant2:path, $variant3:path, $variant4:path, $variant5:path, $($variants:path),+) |$value:ident| $block:block) => {
        match $enum_ {
            $variant1 ($value) => $block,
            $variant2 ($value) => $block,
            $variant3 ($value) => $block,
            $variant4 ($value) => $block,
            $variant5 ($value) => $block,
            _ => execute_on_enum!($enum_; ($($variants),+) |$value| $block),
        }
    };
}

#[allow(clippy::all, clippy::pedantic, clippy::nursery)] //not my code, not my problem
pub mod base64 {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    pub fn base64_encode(data: &[u8]) -> String {
        let mut encoded_string = String::new();
        let mut bits_encoded = 0usize;

        loop {
            let lower_byte_index_to_encode = bits_encoded / 8usize;
            if lower_byte_index_to_encode == data.len() {
                break;
            };

            let lower_byte_to_encode = data[lower_byte_index_to_encode];
            let upper_byte_to_code = if (lower_byte_index_to_encode + 1) == data.len() {
                0u8
            } else {
                data[lower_byte_index_to_encode + 1]
            };

            let bytes_to_encode = (lower_byte_to_encode, upper_byte_to_code);
            let offset: u8 = (bits_encoded % 8) as u8;
            encoded_string
                .push(CHARSET[collect_six_bits(bytes_to_encode, offset) as usize] as char);

            bits_encoded += 6;
        }

        encoded_string
    }

    pub fn base64_decode(data: &str) -> Result<Vec<u8>, (&str, u8)> {
        let mut collected_bits = 0;
        let mut byte_buffer = 0u16;
        let mut databytes = data.bytes();
        let mut outputbytes = Vec::<u8>::new();

        'decodeloop: loop {
            while collected_bits < 8 {
                if let Some(nextbyte) = databytes.next() {
                    if let Some(idx) = CHARSET.iter().position(|&x| x == nextbyte) {
                        byte_buffer |= ((idx & 0b00111111) as u16) << (10 - collected_bits);
                        collected_bits += 6;
                    } else {
                        return Err((
                            "Failed to decode base64: Expected byte from charset, found invalid byte.",
                            nextbyte,
                        ));
                    }
                } else {
                    break 'decodeloop;
                }
            }
            outputbytes.push(((0b1111111100000000 & byte_buffer) >> 8) as u8);
            byte_buffer &= 0b0000000011111111;
            byte_buffer <<= 8;
            collected_bits -= 8;
        }

        if collected_bits != 0 {
            return Err(("Failed to decode base64: Invalid padding.", collected_bits));
        }

        Ok(outputbytes)
    }

    fn collect_six_bits(from: (u8, u8), offset: u8) -> u8 {
        let combined: u16 = ((from.0 as u16) << 8) | (from.1 as u16);
        ((combined & (0b1111110000000000u16 >> offset)) >> (10 - offset)) as u8
    }
}
