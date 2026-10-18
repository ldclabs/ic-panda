use super::*;

fn encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let (mut out, mut buffer, mut bits) = (String::new(), 0u32, 0u32);
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

#[test]
fn reproduces_rfc_6238() {
    let secrets = [
        ("SHA1", "12345678901234567890".to_string()),
        ("SHA256", "12345678901234567890".repeat(2)[..32].to_string()),
        ("SHA512", "1234567890".repeat(7)[..64].to_string()),
    ];
    let cases: [(u64, [&str; 3]); 6] = [
        (59, ["94287082", "46119246", "90693936"]),
        (1_111_111_109, ["07081804", "68084774", "25091201"]),
        (1_111_111_111, ["14050471", "67062674", "99943326"]),
        (1_234_567_890, ["89005924", "91819424", "93441116"]),
        (2_000_000_000, ["69279037", "90698825", "38618901"]),
        (20_000_000_000, ["65353130", "77737706", "47863826"]),
    ];
    for (i, (algorithm, secret)) in secrets.iter().enumerate() {
        let uri = format!(
            "otpauth://totp/ACME:alice?secret={}&algorithm={algorithm}&digits=8&issuer=ACME",
            encode(secret.as_bytes())
        );
        let totp = parse(&uri).unwrap();
        for (seconds, codes) in &cases {
            let code = totp.code(seconds * 1000);
            assert_eq!(code.code, codes[i], "{algorithm} {seconds}");
            assert_eq!(code.period_ms, 30_000);
        }
    }
}

#[test]
fn reads_bare_secrets_and_refuses_the_rest() {
    let totp = parse("jbsw y3dp ehpk 3pxp").unwrap();
    let code = totp.code(59_000);
    assert_eq!((code.code.len(), code.remaining_ms), (6, 1_000));
    assert_eq!(
        parse("JBSWY3DPEHPK3PXP").unwrap().code(59_000).code,
        code.code.clone()
    );
    for value in [
        "",
        "short",
        "otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP&counter=1",
        "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=9",
        "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&algorithm=MD5",
        "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=0",
        "otpauth://totp/x",
        "1234567890123456!",
    ] {
        assert!(parse(value).is_err(), "{value}");
    }
}

#[test]
fn reads_qr_codes() {
    let text = "otpauth://totp/dMsg:alice?secret=JBSWY3DPEHPK3PXP&issuer=dMsg";
    let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
    let (width, scale, quiet) = (code.width() as u32, 6, 4);
    let size = (width + 2 * quiet) * scale;
    let colors = code.to_colors();
    let image = image::GrayImage::from_fn(size, size, |x, y| {
        let (cx, cy) = (x / scale, y / scale);
        let dark = cx >= quiet
            && cy >= quiet
            && cx < quiet + width
            && cy < quiet + width
            && colors[((cy - quiet) * width + (cx - quiet)) as usize] == qrcode::Color::Dark;
        image::Luma([if dark { 0 } else { 255 }])
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    assert_eq!(scan(png.get_ref()).unwrap(), text);

    let blank = image::GrayImage::from_pixel(64, 64, image::Luma([255]));
    let mut png = std::io::Cursor::new(Vec::new());
    blank.write_to(&mut png, image::ImageFormat::Png).unwrap();
    assert_eq!(scan(png.get_ref()).err(), Some(Error::NotFound));
    assert_eq!(
        scan(b"not an image").err(),
        Some(Error::InvalidInput("image".into()))
    );
}
