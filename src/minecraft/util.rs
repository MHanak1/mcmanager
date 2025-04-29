use std::collections::HashMap;

pub fn parse_minecraft_properties(file: &str) -> HashMap<String, String> {
    let mut properties = HashMap::new();
    for line in file.lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut split = line.splitn(2, '=');
        let key = split.next().unwrap_or("");
        let value = split.next().unwrap_or("");
        properties.insert(key.to_string(), value.to_string());
    }
    properties
}

pub fn create_minecraft_properties(properties: HashMap<String, String>) -> String {
    let mut file = String::new();
    for (key, value) in properties {
        file.push_str(&format!("{}={}\n", key, value));
    }
    file
}