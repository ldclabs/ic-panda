use base64::{engine::general_purpose, Engine};
use finl_unicode::categories::CharacterCategories;

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

pub fn luck_amount(luck: u64, avg: u64, num: u64) -> u64 {
    if num <= 1 {
        return avg;
    }
    let luck = 1 + (luck / 100);
    let r = match luck {
        0..=10000 => 1.0 + (luck as f32).ln(),
        _ => 10.22f32,
    };
    let r = r * r / 10f32;
    let v = avg as f32 * r;
    let max = match num {
        2 => 0.75f32,
        3 => 0.67f32,
        _ => 0.5f32,
    };
    let max = max * (avg * num) as f32;
    v.min(max).round() as u64
}

// name should be valid utf-8 string, not empty, not longer than 64 bytes, and not contain any of the following characters: uppercase letters, punctuations, separators, marks, symbols, and other control characters, format characters, surrogates, unassigned characters and private use characters.
// https://docs.rs/finl_unicode/latest/finl_unicode/categories/trait.CharacterCategories.html
pub fn valid_name(name: &str) -> Result<(), String> {
    let mut size = 0;
    // let cs = Graphemes::new(name);

    if name.is_empty() {
        return Err("name should not be empty".to_string());
    }

    for c in name.chars() {
        size += 1;
        if size > 32 {
            return Err("name should be less than 32 chars".to_string());
        }
        if c.is_letter_uppercase()
            || c.is_punctuation()
            || c.is_mark()
            || c.is_symbol()
            || c.is_other()
            || (!c.is_separator_space() && c.is_separator())
        {
            return Err(format!("name contains invalid character: {:?}", c));
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_lucky_code() {
        for code in [
            u32::MIN,
            9,
            1000000 - 1,
            1000000,
            0x12345678,
            0x87654321,
            u32::MAX,
        ]
        .iter()
        {
            let s = luckycode_to_string(*code); // 0, "AAAAAA"
            let c = luckycode_from_string(&s).unwrap();
            println!("code: {}, {}", s, c);
            assert_eq!(code, &c);
        }
    }

    #[test]
    fn test_lucky_amount() {
        let avg = 100;
        for luck in [
            10, 100, 200, 300, 500, 900, 1000, 1200, 1500, 1800, 2000, 5000, 10000, 20000, 50000,
            100000, 500000, 1000000,
        ]
        .iter()
        {
            for num in [2, 3, 5, 10, 20, 50, 100].iter() {
                let amount = luck_amount(*luck, avg, *num);
                println!(
                    "Lucky balance: {}, claim amount up to: {} * avg",
                    luck,
                    amount as f32 / avg as f32
                );
                assert!(amount >= avg / 10);
                assert!(amount <= avg * 11);
            }
            // Lucky balance: 10, claim amount up to: 0.1 * avg
            // Lucky balance: 100, claim amount up to: 0.29 * avg
            // Lucky balance: 200, claim amount up to: 0.44 * avg
            // Lucky balance: 300, claim amount up to: 0.57 * avg
            // Lucky balance: 500, claim amount up to: 0.78 * avg
            // Lucky balance: 900, claim amount up to: 1.09 * avg
            // Lucky balance: 1000, claim amount up to: 1.15 * avg
            // Lucky balance: 1200, claim amount up to: 1.27 * avg
            // Lucky balance: 1500, claim amount up to: 1.42 * avg
            // Lucky balance: 1800, claim amount up to: 1.56 * avg
            // Lucky balance: 2000, claim amount up to: 1.64 * avg
            // Lucky balance: 5000, claim amount up to: 2.43 * avg
            // Lucky balance: 10000, claim amount up to: 3.15 * avg
            // Lucky balance: 20000, claim amount up to: 3.97 * avg
            // Lucky balance: 50000, claim amount up to: 5.21 * avg
            // Lucky balance: 100000, claim amount up to: 6.25 * avg
            // Lucky balance: 500000, claim amount up to: 9.06 * avg
            // Lucky balance: 1000000, claim amount up to: 10.44 * avg
        }
    }
}
