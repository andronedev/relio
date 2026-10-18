use serde::Serialize;

/// Parsed from `.kobo/version`: `serial,kernel,firmware,_,_,model_id`.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub model_id: Option<String>,
    pub model: String,
}

impl DeviceInfo {
    pub fn parse(version_file: &str) -> Self {
        let fields: Vec<&str> = version_file.trim().split(',').map(str::trim).collect();
        let field = |i: usize| fields.get(i).filter(|s| !s.is_empty()).map(|s| (*s).to_owned());
        let model_id = field(5);
        let model = model_id
            .as_deref()
            .and_then(model_name)
            .unwrap_or("Kobo")
            .to_owned();
        Self {
            serial: field(0),
            firmware: field(2),
            model_id,
            model,
        }
    }

    pub fn unknown() -> Self {
        Self {
            serial: None,
            firmware: None,
            model_id: None,
            model: "Kobo".to_owned(),
        }
    }
}

fn model_name(model_id: &str) -> Option<&'static str> {
    let suffix = model_id.rsplit('-').next()?.trim_start_matches('0');
    Some(match suffix {
        "310" => "Kobo Touch A/B",
        "320" => "Kobo Touch C",
        "330" => "Kobo Glo",
        "340" => "Kobo Mini",
        "350" => "Kobo Aura HD",
        "360" => "Kobo Aura",
        "370" => "Kobo Aura H2O",
        "371" => "Kobo Glo HD",
        "372" => "Kobo Touch 2.0",
        "373" => "Kobo Aura ONE",
        "374" => "Kobo Aura H2O Edition 2 v1",
        "375" => "Kobo Aura Edition 2 v1",
        "376" => "Kobo Clara HD",
        "377" => "Kobo Forma",
        "378" => "Kobo Aura H2O Edition 2 v2",
        "379" => "Kobo Aura Edition 2 v2",
        "380" => "Kobo Forma 32GB",
        "381" => "Kobo Aura ONE Limited Edition",
        "382" => "Kobo Nia",
        "383" => "Kobo Sage",
        "384" => "Kobo Libra H2O",
        "386" => "Kobo Clara 2E",
        "387" => "Kobo Elipsa",
        "388" => "Kobo Libra 2",
        "389" => "Kobo Elipsa 2E",
        "390" => "Kobo Libra Colour",
        "391" => "Kobo Clara BW",
        "393" => "Kobo Clara Colour",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clara_colour() {
        let info = DeviceInfo::parse(
            "N3670A0000000,4.9.77,4.44.23552,4.9.77,4.9.77,00000000-0000-0000-0000-000000000393\n",
        );
        assert_eq!(info.model, "Kobo Clara Colour");
        assert_eq!(info.firmware.as_deref(), Some("4.44.23552"));
    }

    #[test]
    fn tolerates_garbage() {
        assert_eq!(DeviceInfo::parse("").model, "Kobo");
    }
}
