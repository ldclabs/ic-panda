use super::*;
use crate::{crypto, vault::backup::content};
use std::io::Cursor;

const FIXTURE: &[u8] =
    include_bytes!("../../../../../tests/vectors/client/vault-backup-v1.dmsgvault");

#[test]
fn independent_full_archive_matches_the_writer_and_existing_encrypt0_codec() {
    let (body, digest) = content::read(FIXTURE, "123456", |_| Ok(()), || Ok(())).unwrap();
    assert_eq!(
        digest,
        "db4bac2a8cbe77f1feb6b3e35fb93d1ccb42b893603a177f2ccf40c89eec1b51"
    );
    assert_eq!(
        body.entries[1].fields[0].value,
        "synthetic non-wallet test words"
    );
    assert_eq!(body.meta.item_count, 2);
    let mut input = Cursor::new(FIXTURE);
    assert_eq!(length(&mut input, 4, 3).unwrap(), 3);
    let (h, _): (Header, _) = record(&mut input, 128).unwrap();
    let wrap = bytes(&mut input, 128).unwrap();
    let derived = derive("123456", &h.2[..]).unwrap();
    let key = crypto::open_bytes(&derived, &wrap, &(WRAP, &h)).unwrap();
    assert_eq!(length(&mut input, 4, 1).unwrap(), 1);
    let block = bytes(&mut input, CHUNK + 64).unwrap();
    let raw = crypto::open_bytes(
        key.as_slice().try_into().unwrap(),
        &block,
        &(BLOCK, &h, 0u64),
    )
    .unwrap();
    let mut writer = Writer::with_keys(
        Vec::new(),
        raw.len(),
        "123456",
        *h.2,
        &(16u8..28).collect::<Vec<_>>(),
        key.as_slice().try_into().unwrap(),
    )
    .unwrap();
    writer.push(&raw).unwrap();
    assert_eq!(writer.finish().unwrap(), FIXTURE);
}

#[test]
fn block_order_and_the_authenticated_end_are_mandatory() {
    let input = vec![0x73; 2 * CHUNK];
    let mut writer = Writer::new(Vec::new(), input.len(), "123456").unwrap();
    writer.push(&input).unwrap();
    let raw = writer.finish().unwrap();
    let (h, wrapped, blocks): (Header, serde_bytes::ByteBuf, Vec<serde_bytes::ByteBuf>) =
        decode(&raw, raw.len()).unwrap();
    let check = |data: Vec<u8>| {
        let mut reader = Reader::new(data.as_slice(), "123456")?;
        let mut dest = Vec::new();
        reader.read_to_end(&mut dest).map_err(file_io)?;
        reader.finish()?;
        Ok::<_, Error>(dest)
    };
    assert_eq!(check(raw.clone()).unwrap(), input);
    let mut swapped = blocks.clone();
    swapped.swap(0, 1);
    assert!(check(canonical(&(&h, &wrapped, swapped))).is_err());
    assert!(check(canonical(&(&h, &wrapped, &blocks[..1]))).is_err());
    assert!(check(canonical(&(&h, &wrapped, [&blocks[0], &blocks[0]]))).is_err());
    let mut trailing = raw;
    trailing.push(0);
    assert!(check(trailing).is_err());
}
