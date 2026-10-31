//! The quantised network almoNnuet's `export.py` writes (training spec §6).

use std::fmt;
use std::path::{Path, PathBuf};

pub const L0: usize = 41_024;
pub const L1: usize = 256;
pub const L2: usize = 32;
pub const L3: usize = 32;
pub const QA: i32 = 127;
pub const QB: i32 = 64;
pub const OUT_SCALE: i32 = 600;

/// What `feature_set_hash` in the header is the FNV-1a hash of.
pub const FEATURE_SET: &str = "HalfKP-v1;K=64;P=641;orient=xor56;pidx=2*pt+(c!=p);pad=0;stm_first";
/// What `arch_hash` in the header is the FNV-1a hash of.
pub const ARCH: &str = "41024-256-32-32-1";

const MAGIC: [u8; 4] = *b"ALMN";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 32;
/// `l0`, `l1`, `l2`, `l3`, `QA`, `QB`, `out_scale`, as the header stores them.
const DIMS: [u32; 7] = [41_024, 256, 32, 32, 127, 64, 600];
/// Bound for random biases behind the hidden layers: `QA * QB`.
const BIAS_BOUND: u16 = 8128;

/// Header plus every tensor: 21,022,564 bytes.
pub const FILE_SIZE: usize =
    HEADER_SIZE + L0 * L1 * 2 + L1 * 2 + L2 * 2 * L1 + L2 * 4 + L3 * L2 + L3 * 4 + L3 * 2 + 4;

/// 32-bit FNV-1a, as almoNnuet's `features.fnv1a32`.
pub fn fnv1a32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811C_9DC5, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

/// Every tensor of the net, with its final integer type.
pub struct Network {
    /// `L0` rows of `L1`, feature-major: row `f` holds feature `f`'s weights.
    pub ft_weight: Box<[i16]>,
    pub ft_bias: [i16; L1],
    /// `L2` rows of `2 * L1`: row `o` holds output `o`'s weights, stm half first.
    pub l1_weight: Box<[i8]>,
    pub l1_bias: [i32; L2],
    /// `L3` rows of `L2`.
    pub l2_weight: Box<[i8]>,
    pub l2_bias: [i32; L3],
    pub out_weight: [i16; L3],
    pub out_bias: i32,
}

impl fmt::Debug for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Network")
            .field("out_bias", &self.out_bias)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0} bytes, expected {expected}", expected = FILE_SIZE)]
    Size(usize),
    #[error("not an .nnue file: magic {0:?}")]
    Magic([u8; 4]),
    #[error("unsupported .nnue version {0}")]
    Version(u32),
    #[error("feature-set hash {0:#010x} is not HalfKP-v1")]
    FeatureSet(u32),
    #[error("architecture hash {0:#010x} is not {arch}", arch = ARCH)]
    Arch(u32),
    #[error("layer sizes and scales {0:?}, expected {expected:?}", expected = DIMS)]
    Dims([u32; 7]),
}

impl Network {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| LoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LoadError> {
        if bytes.len() < HEADER_SIZE {
            return Err(LoadError::Size(bytes.len()));
        }
        let mut r = Reader { bytes, at: 0 };
        let magic: [u8; 4] = r.take(4).try_into().expect("4 bytes");
        if magic != MAGIC {
            return Err(LoadError::Magic(magic));
        }
        let version = r.u32();
        if version != VERSION {
            return Err(LoadError::Version(version));
        }
        let feature_set = r.u32();
        if feature_set != fnv1a32(FEATURE_SET.as_bytes()) {
            return Err(LoadError::FeatureSet(feature_set));
        }
        let arch = r.u32();
        if arch != fnv1a32(ARCH.as_bytes()) {
            return Err(LoadError::Arch(arch));
        }
        let dims = [
            u32::from(r.u16()),
            u32::from(r.u16()),
            u32::from(r.u16()),
            u32::from(r.u16()),
            u32::from(r.u8()),
            u32::from(r.u8()),
            u32::from(r.u16()),
        ];
        if dims != DIMS {
            return Err(LoadError::Dims(dims));
        }
        if bytes.len() != FILE_SIZE {
            return Err(LoadError::Size(bytes.len()));
        }
        r.at = HEADER_SIZE;
        Ok(Self {
            ft_weight: r.i16s(L0 * L1).into_boxed_slice(),
            ft_bias: array(r.i16s(L1)),
            l1_weight: r.i8s(L2 * 2 * L1).into_boxed_slice(),
            l1_bias: array(r.i32s(L2)),
            l2_weight: r.i8s(L3 * L2).into_boxed_slice(),
            l2_bias: array(r.i32s(L3)),
            out_weight: array(r.i16s(L3)),
            out_bias: r.i32s(1)[0],
        })
    }

    /// A deterministic random net within the training clips (|ft| <= 254,
    /// |hidden| <= 127), for tests that must not need the real file.
    pub fn random(seed: u64) -> Self {
        let mut rng = SplitMix(seed);
        let mut ft_weight: Vec<i16> = (0..L0 * L1).map(|_| rng.range(254) as i16).collect();
        ft_weight[..L1].fill(0); // feature 0 is padding, as in training
        Self {
            ft_weight: ft_weight.into_boxed_slice(),
            ft_bias: std::array::from_fn(|_| rng.range(127) as i16),
            l1_weight: (0..L2 * 2 * L1).map(|_| rng.range(127) as i8).collect(),
            l1_bias: std::array::from_fn(|_| rng.range(BIAS_BOUND)),
            l2_weight: (0..L3 * L2).map(|_| rng.range(127) as i8).collect(),
            l2_bias: std::array::from_fn(|_| rng.range(BIAS_BOUND)),
            out_weight: std::array::from_fn(|_| rng.range(127) as i16),
            out_bias: rng.range(BIAS_BOUND),
        }
    }

    /// The `L1` feature-transformer weights of `feature`: one contiguous row.
    pub fn ft_row(&self, feature: usize) -> &[i16; L1] {
        self.ft_weight[feature * L1..(feature + 1) * L1]
            .try_into()
            .expect("rows are L1 long")
    }
}

fn array<T: fmt::Debug, const N: usize>(values: Vec<T>) -> [T; N] {
    values.try_into().expect("length checked against FILE_SIZE")
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> &'a [u8] {
        let slice = &self.bytes[self.at..self.at + n];
        self.at += n;
        slice
    }

    fn u8(&mut self) -> u8 {
        self.take(1)[0]
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.take(2).try_into().expect("2 bytes"))
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().expect("4 bytes"))
    }

    fn i8s(&mut self, n: usize) -> Vec<i8> {
        self.take(n)
            .iter()
            .map(|&b| i8::from_le_bytes([b]))
            .collect()
    }

    fn i16s(&mut self, n: usize) -> Vec<i16> {
        self.take(2 * n)
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect()
    }

    fn i32s(&mut self, n: usize) -> Vec<i32> {
        self.take(4 * n)
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes(c.try_into().expect("4 bytes")))
            .collect()
    }
}

/// `SplitMix64`: small, seedable, good enough for test nets.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `-bound..=bound`.
    fn range(&mut self, bound: u16) -> i32 {
        let span = u64::from(bound) * 2 + 1;
        i32::try_from(self.next() % span).expect("span fits i32") - i32::from(bound)
    }
}
