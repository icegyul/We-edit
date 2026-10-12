//! API keys of cloud services. They live only in the Windows Credential Manager (the system's
//! own store, protected by the user's login): never in the settings file, a project, the logs or
//! this program's folder.

const SERVICE: &str = "We edit";

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, name).map_err(|e| e.to_string())
}

/// The saved key, or None when there is none (or the store cannot be read).
pub fn get(name: &str) -> Option<String> {
    entry(name).ok()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn set(name: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("The key is empty".into());
    }
    entry(name)?.set_password(key).map_err(|e| format!("The key could not be saved: {e}"))
}

pub fn delete(name: &str) -> Result<(), String> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_goes_into_the_system_store_and_comes_out_again() {
        let name = format!("test-key-{}", std::process::id());
        assert_eq!(get(&name), None);
        set(&name, "  sk-test-1234  ").unwrap();
        assert_eq!(get(&name).as_deref(), Some("sk-test-1234"));
        delete(&name).unwrap();
        assert_eq!(get(&name), None);
        // deleting what is not there is not an error
        delete(&name).unwrap();
        assert!(set(&name, "   ").is_err());
    }
}
