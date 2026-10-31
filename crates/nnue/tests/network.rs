use nnue::{ARCH, FEATURE_SET, FILE_SIZE, L0, L1, L2, L3, LoadError, Network, fnv1a32};

const OFF_FT_BIAS: usize = 32 + L0 * L1 * 2;
const OFF_L1_WEIGHT: usize = OFF_FT_BIAS + L1 * 2;
const OFF_L1_BIAS: usize = OFF_L1_WEIGHT + L2 * 2 * L1;
const OFF_L2_WEIGHT: usize = OFF_L1_BIAS + L2 * 4;
const OFF_L2_BIAS: usize = OFF_L2_WEIGHT + L3 * L2;
const OFF_OUT_WEIGHT: usize = OFF_L2_BIAS + L3 * 4;
const OFF_OUT_BIAS: usize = OFF_OUT_WEIGHT + L3 * 2;

/// The header almoNnuet writes, built by hand from the training spec.
fn header() -> Vec<u8> {
    let mut h = Vec::with_capacity(32);
    h.extend_from_slice(b"ALMN");
    h.extend_from_slice(&1u32.to_le_bytes());
    h.extend_from_slice(&0x1C61_3D35u32.to_le_bytes());
    h.extend_from_slice(&0x9F64_C9B6u32.to_le_bytes());
    for dim in [41_024u16, 256, 32, 32] {
        h.extend_from_slice(&dim.to_le_bytes());
    }
    h.extend_from_slice(&[127, 64]);
    h.extend_from_slice(&600u16.to_le_bytes());
    h.extend_from_slice(&[0; 4]);
    h
}

fn blank_file() -> Vec<u8> {
    let mut file = header();
    file.resize(FILE_SIZE, 0);
    file
}

fn put(file: &mut [u8], at: usize, bytes: &[u8]) {
    file[at..at + bytes.len()].copy_from_slice(bytes);
}

#[test]
fn file_size_is_the_one_almonnuet_writes() {
    assert_eq!(FILE_SIZE, 21_022_564);
    assert_eq!(OFF_OUT_BIAS + 4, FILE_SIZE);
}

#[test]
fn fnv1a32_known_vectors() {
    assert_eq!(fnv1a32(b""), 0x811C_9DC5);
    assert_eq!(fnv1a32(b"a"), 0xE40C_292C);
    assert_eq!(fnv1a32(b"foobar"), 0xBF9C_F968);
}

#[test]
fn header_hashes_match_almonnuet() {
    assert_eq!(fnv1a32(FEATURE_SET.as_bytes()), 0x1C61_3D35);
    assert_eq!(fnv1a32(ARCH.as_bytes()), 0x9F64_C9B6);
}

#[test]
fn a_blank_file_loads_as_zeros() {
    let net = Network::from_bytes(&blank_file()).unwrap();
    assert!(net.ft_weight.iter().all(|&w| w == 0));
    assert_eq!(net.ft_weight.len(), L0 * L1);
    assert_eq!(net.out_bias, 0);
}

#[test]
fn every_tensor_is_read_from_its_offset() {
    let mut file = blank_file();
    put(&mut file, 32 + (7 * L1 + 3) * 2, &(-300i16).to_le_bytes());
    put(&mut file, OFF_FT_BIAS + 5 * 2, &42i16.to_le_bytes());
    put(
        &mut file,
        OFF_L1_WEIGHT + 2 * 2 * L1 + 9,
        &(-5i8).to_le_bytes(),
    );
    put(&mut file, OFF_L1_BIAS + 2 * 4, &1016i32.to_le_bytes());
    put(
        &mut file,
        OFF_L2_WEIGHT + 4 * L2 + 5,
        &(-48i8).to_le_bytes(),
    );
    put(&mut file, OFF_L2_BIAS + 4 * 4, &(-1524i32).to_le_bytes());
    put(&mut file, OFF_OUT_WEIGHT + 6 * 2, &96i16.to_le_bytes());
    put(&mut file, OFF_OUT_BIAS, &2540i32.to_le_bytes());
    let net = Network::from_bytes(&file).unwrap();
    assert_eq!(net.ft_row(7)[3], -300);
    assert_eq!(net.ft_bias[5], 42);
    assert_eq!(net.l1_weight[2 * 2 * L1 + 9], -5);
    assert_eq!(net.l1_bias[2], 1016);
    assert_eq!(net.l2_weight[4 * L2 + 5], -48);
    assert_eq!(net.l2_bias[4], -1524);
    assert_eq!(net.out_weight[6], 96);
    assert_eq!(net.out_bias, 2540);
}

/// The error a valid file gives once `bytes` overwrite it at `at`.
fn corrupted(at: usize, bytes: &[u8]) -> LoadError {
    let mut file = blank_file();
    put(&mut file, at, bytes);
    Network::from_bytes(&file).unwrap_err()
}

#[test]
fn corrupt_headers_are_rejected() {
    let magic = corrupted(0, b"XXXX");
    assert!(
        matches!(magic, LoadError::Magic(m) if &m == b"XXXX"),
        "{magic}"
    );
    let version = corrupted(4, &2u32.to_le_bytes());
    assert!(matches!(version, LoadError::Version(2)), "{version}");
    let feature_set = corrupted(8, &0u32.to_le_bytes());
    assert!(
        matches!(feature_set, LoadError::FeatureSet(0)),
        "{feature_set}"
    );
    let arch = corrupted(12, &0u32.to_le_bytes());
    assert!(matches!(arch, LoadError::Arch(0)), "{arch}");
    let dims = corrupted(18, &128u16.to_le_bytes());
    assert!(matches!(dims, LoadError::Dims(_)), "{dims}");
}

#[test]
fn truncated_and_short_files_are_rejected() {
    let file = blank_file();
    let truncated = Network::from_bytes(&file[..FILE_SIZE - 1]).unwrap_err();
    assert!(matches!(truncated, LoadError::Size(n) if n == FILE_SIZE - 1));
    let short = Network::from_bytes(&file[..10]).unwrap_err();
    assert!(matches!(short, LoadError::Size(10)));
}

#[test]
fn a_missing_file_names_its_path() {
    let err = Network::load("/no/such/dir/net.nnue").unwrap_err();
    assert!(matches!(err, LoadError::Io { .. }));
    assert!(err.to_string().contains("/no/such/dir/net.nnue"), "{err}");
}

#[test]
fn random_nets_are_deterministic_and_within_the_training_clips() {
    let a = Network::random(1);
    assert_eq!(a.ft_weight, Network::random(1).ft_weight);
    assert_ne!(a.ft_weight, Network::random(2).ft_weight);
    assert!(a.ft_weight.iter().all(|w| w.unsigned_abs() <= 254));
    assert!(a.ft_row(0).iter().all(|&w| w == 0), "feature 0 is padding");
    assert!(a.l1_weight.iter().all(|w| w.unsigned_abs() <= 127));
    assert!(a.l2_weight.iter().all(|w| w.unsigned_abs() <= 127));
}
