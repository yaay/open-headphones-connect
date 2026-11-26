//! What can be told from a headset's Bluetooth name alone. These are first
//! guesses, used until the headset reports its own capabilities.

use crate::protocol::Generation;

/// Which generation of the protocol a device of this name speaks, or `None`
/// when the name is not a Sony headset's. For when the operating system has
/// no service record to go by.
pub fn generation_from_name(name: &str) -> Option<Generation> {
    let upper = name.to_ascii_uppercase();
    let sony = ["WH-", "WF-", "MDR-", "LINKBUDS", "ULT WEAR", "INZONE"]
        .iter()
        .any(|prefix| upper.starts_with(prefix) || upper.starts_with(&format!("LE_{prefix}")));
    if !sony {
        return None;
    }
    let first_gen = ["XM3", "WH-1000XM4", "WH-1000XM2", "MDR-1000X", "H900N", "H910N", "XB900N", "CH710N"]
        .iter()
        .any(|model| upper.contains(model));
    Some(if first_gen { Generation::V1 } else { Generation::V2 })
}

pub fn has_wind_mode(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["WH-1000XM3", "WH-1000XM2", "MDR-1000X"].iter().any(|m| upper.contains(m))
}

pub fn is_earbuds(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.contains("WF-")
        || upper.contains("LINKBUDS")
        || crate::protocol::nothing::Model::from_name(name).is_some_and(|model| model.earbuds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_generation_only_for_sony_names() {
        assert_eq!(generation_from_name("WH-1000XM3"), Some(Generation::V1));
        assert_eq!(generation_from_name("LE_WH-1000XM3"), Some(Generation::V1));
        assert_eq!(generation_from_name("WH-1000XM4"), Some(Generation::V1));
        assert_eq!(generation_from_name("WF-1000XM4"), Some(Generation::V2));
        assert_eq!(generation_from_name("WH-1000XM5"), Some(Generation::V2));
        assert_eq!(generation_from_name("LinkBuds S"), Some(Generation::V2));
        assert_eq!(generation_from_name("AirPods Pro"), None);
        assert_eq!(generation_from_name("Logitech MX Keys"), None);
    }

    #[test]
    fn knows_which_models_have_a_wind_mode() {
        assert!(has_wind_mode("WH-1000XM3"));
        assert!(!has_wind_mode("WH-1000XM4"));
        assert!(is_earbuds("WF-1000XM3"));
        assert!(!is_earbuds("WH-1000XM3"));
    }
}
