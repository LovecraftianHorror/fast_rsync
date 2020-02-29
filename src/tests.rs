use quickcheck_macros::quickcheck;
use std::io::Cursor;

use crate::{apply, diff, Signature, SignatureOptions};

#[quickcheck]
fn test_signature_creation(data: Vec<u8>, block_size: u32, crypto_hash_size: u32) {
    let mut storage = Vec::new();
    let signature = Signature::calculate(
        &data,
        &mut storage,
        SignatureOptions {
            block_size: block_size + 1,
            crypto_hash_size: crypto_hash_size % 16,
        },
    );
    let mut serialized = Vec::new();
    signature.serialize(&mut serialized);
    let deserialized = Signature::deserialize(&serialized).expect("deserialization error");
    assert_eq!(signature, deserialized);
}

#[test]
fn test_trivial() {
    let data = vec![0; 100000];
    let mut storage = Vec::new();
    let signature = Signature::calculate(
        &data,
        &mut storage,
        SignatureOptions {
            block_size: 64,
            crypto_hash_size: 5,
        },
    );
    let indexed = signature.index();
    let mut patch = vec![];
    diff(&indexed, &data, &mut patch).expect("diff error");
    let mut out = vec![];
    assert!(patch.len() < 10000);
    apply(&data, &patch, &mut out).expect("apply error");
    assert_eq!(data, out);
}

#[test]
fn test_delta_size() {
    let mut data1 = b"hello".to_vec();
    let mut data2 = b"goodbye".to_vec();
    data1.resize(1 << 20, 0);
    data2.resize(1 << 20, 0);

    let mut storage = Vec::new();
    let signature = Signature::calculate(
        &data1,
        &mut storage,
        SignatureOptions {
            block_size: 4096,
            crypto_hash_size: 8,
        },
    );
    let mut patch = vec![];
    diff(&signature.index(), &data2, &mut patch).expect("diff error");
    let mut out = vec![];
    assert!(patch.len() < 10000);
    apply(&data1, &patch, &mut out).expect("apply error");
    assert_eq!(data2, out);
}

#[test]
fn test_random() {
    use rand::Rng;
    let mut base = vec![0; 100000];
    rand::thread_rng().fill(&mut base[..]);
    let mut data = vec![0; 100000];
    rand::thread_rng().fill(&mut data[..]);
    let mut storage = Vec::new();
    let signature = Signature::calculate(
        &base,
        &mut storage,
        SignatureOptions {
            block_size: 4,
            crypto_hash_size: 8,
        },
    );
    let indexed = signature.index();
    let mut patch = vec![];
    diff(&indexed, &data, &mut patch).expect("diff error");
    let mut out = vec![];
    apply(&base, &patch, &mut out).expect("apply error");
    assert_eq!(data, out);

    // interoperability: we can apply patches generated by librsync
    let mut serialized_signature = vec![];
    signature.serialize(&mut serialized_signature);
    let mut librsync_patch = vec![];
    librsync::whole::delta(
        &mut &data[..],
        &mut &serialized_signature[..],
        &mut librsync_patch,
    )
    .unwrap();
    out.clear();
    apply(&base, &librsync_patch, &mut out).expect("apply error");
    assert_eq!(data, out);

    // interoperability: librsync can apply our patches
    out.clear();
    librsync::whole::patch(&mut Cursor::new(&base[..]), &mut &patch[..], &mut out).unwrap();
    assert_eq!(data, out);
}

#[test]
fn test_signature_interoperability() {
    // interoperability: we generate identical signatures to librsync
    use rand::Rng;
    for &block_len in &[10, 1024] {
        for &strong_len in &[1, 8, 16] {
            for &len in &[0, 1, 2, 10, 128, 500, 1111, 2000, 2048] {
                let mut data = vec![0; len];
                rand::thread_rng().fill(&mut data[..]);
                let mut librsync_out = vec![];
                librsync::whole::signature_with_options(
                    &mut &data[..],
                    &mut librsync_out,
                    block_len,
                    strong_len,
                    librsync::SignatureType::MD4,
                )
                .unwrap();
                let mut storage = Vec::new();
                let signature = Signature::calculate(
                    &data,
                    &mut storage,
                    SignatureOptions {
                        block_size: block_len as u32,
                        crypto_hash_size: strong_len as u32,
                    },
                );
                let mut serialized = Vec::new();
                signature.serialize(&mut serialized);
                assert_eq!(
                    librsync_out, serialized,
                    "block_len={}, strong_len={}, len={}",
                    block_len, strong_len, len
                );
            }
        }
    }
}

#[test]
fn test_apply_errors() {
    let base_data = b"potato";
    // sanity check: empty patch
    apply(
        base_data,
        &[114, 115, 2, 54, 0],
        &mut Vec::new(),
    ).unwrap();
    // no magic
    apply(base_data, &[], &mut Vec::new()).unwrap_err();
    // wrong magic
    apply(base_data, &[1, 2, 3, 4], &mut Vec::new()).unwrap_err();
    // zero-length copy
    apply(
        base_data,
        &[114, 115, 2, 54, crate::consts::RS_OP_COPY_N1_N1, 0, 0, 0],
        &mut Vec::new(),
    )
    .unwrap_err();
    // copy start out of range
    apply(
        base_data,
        &[114, 115, 2, 54, crate::consts::RS_OP_COPY_N1_N1, 10, 1, 0],
        &mut Vec::new(),
    )
    .unwrap_err();
    // copy end out of range
    apply(
        base_data,
        &[114, 115, 2, 54, crate::consts::RS_OP_COPY_N1_N1, 0, 10, 0],
        &mut Vec::new(),
    )
    .unwrap_err();
    // copy end out of range
    apply(
        base_data,
        &[114, 115, 2, 54, crate::consts::RS_OP_COPY_N1_N1, 0, 10, 0],
        &mut Vec::new(),
    )
        .unwrap_err();
}
