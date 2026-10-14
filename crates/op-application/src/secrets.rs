//! API keys of cloud services. They live only in the Windows Credential Manager (the system's
//! own store, protected by the user's login): never in the settings file, a project, the logs or
//! this program's folder.

const SERVICE: &str = "We edit";

/// A video service whose key is kept here. Making videos with it is not built yet: only the key.
pub struct VideoService {
    pub label: &'static str,
    /// The name the key is stored under in the Credential Manager.
    pub secret: &'static str,
    /// Where the key is made (empty when there is no page to link).
    pub page: &'static str,
    pub note: &'static str,
}

pub const VIDEO_SERVICES: [VideoService; 2] = [
    VideoService {
        label: "클링 (Kling) API 키",
        secret: "kling-api-key",
        page: "https://kling.ai/dev/api-key",
        note: "발급 후 한 번만 보이니 바로 복사해 두세요. 기존 방식(Access Key + Secret Key)은 아직 넣지 않습니다.",
    },
    VideoService {
        label: "씨댄스 (Seedance, BytePlus ModelArk) API 키",
        secret: "seedance-api-key",
        page: "",
        note: "BytePlus ModelArk 콘솔의 API 키 메뉴에서 발급합니다. 해외 BytePlus 계정 기준입니다.",
    },
];

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
    fn video_services_have_distinct_secret_names() {
        let names: Vec<&str> = VIDEO_SERVICES.iter().map(|s| s.secret).collect();
        assert_eq!(names.len(), 2);
        assert_ne!(names[0], names[1]);
        assert!(VIDEO_SERVICES.iter().all(|s| !s.label.is_empty() && !s.note.is_empty()));
    }

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
