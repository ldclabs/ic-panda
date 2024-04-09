use base64::{engine::general_purpose, Engine};

pub fn luckycode_to_string(code: u32) -> String {
    general_purpose::URL_SAFE_NO_PAD.encode(code.to_be_bytes())
}

pub fn luckycode_from_string(code: &str) -> Result<u32, String> {
    let code = general_purpose::URL_SAFE_NO_PAD
        .decode(code.as_bytes())
        .map_err(|_err| "invalid lucky code".to_string())?;
    if code.len() != 4 {
        return Err("invalid lucky code".to_string());
    }
    Ok(u32::from_be_bytes([code[0], code[1], code[2], code[3]]))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_luckycode() {
        for code in [u32::MIN, 9, 0x12345678, 0x87654321, u32::MAX].iter() {
            let s = luckycode_to_string(*code); // 0, "AAAAAA"
            let c = luckycode_from_string(&s).unwrap();
            assert_eq!(code, &c);
        }
    }
}
