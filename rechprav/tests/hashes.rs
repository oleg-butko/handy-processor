use sha2::{Digest, Sha256};

#[test]
fn sha256_matches_independent_vectors_across_padding_and_stream_boundaries() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../data/fixtures/hash-vectors.json")).unwrap();
    for vector in fixture["vectors"].as_array().unwrap() {
        let length = vector["length"].as_u64().unwrap() as usize;
        let data: Vec<_> = (0..length).map(|i| ((i * 37 + 11) % 256) as u8).collect();
        let expected = vector["sha256"].as_str().unwrap();
        assert_eq!(
            rechprav::hashing::sha256_hex(&data),
            expected,
            "length={length}"
        );
        for size in [1, 7, 63, 64, 65, 1024] {
            let mut hasher = Sha256::new();
            for chunk in data.chunks(size) {
                hasher.update(chunk);
            }
            assert_eq!(
                rechprav::hashing::hex(&hasher.finalize()),
                expected,
                "length={length}, chunk={size}"
            );
        }
    }
}
