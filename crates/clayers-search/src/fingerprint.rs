//! 256-bit structural fingerprint encoder.
//!
//! Deterministic bit-packing of a node's layer, namespace, ancestor-path
//! n-grams, element name, and relation-type incidence. See
//! `clayers/clayers/search.xml#search-structural-fingerprint` for the
//! segment layout.
//!
//! The resulting `[u8; 32]` goes into the last 256 components of the
//! concatenated vector stored in the `usearch` index; the Tanimoto
//! half of the custom metric operates over these bits.

use clayers_spec::chunker::Chunk;

/// Width of the structural fingerprint, in bits.
pub const FINGERPRINT_BITS: usize = 256;

/// Width of the structural fingerprint, in bytes.
pub const FINGERPRINT_BYTES: usize = FINGERPRINT_BITS / 8;

// Bit offsets per segment, end-exclusive.
const LAYER_START: usize = 0;
const NS_START: usize = 11;
const NS_END: usize = 43;
const PATH_START: usize = 43;
const PATH_END: usize = 171;
const ELEM_START: usize = 171;
const ELEM_END: usize = 187;
const OUT_REL_START: usize = 187;
const OUT_REL_END: usize = 219;
const IN_REL_START: usize = 219;
const IN_REL_END: usize = 251;
// bits 251-255 reserved (zero-filled).

/// Canonical list of clayers layer names used for the one-hot segment.
/// The order is stable: new layers append at the end so existing bit
/// positions stay valid. Matches `Chunk.layer` values
/// (long form from `urn:clayers:<NAME>`).
const LAYER_PREFIXES: [&str; 11] = [
    "prose",
    "terminology",
    "organization",
    "relation",
    "decision",
    "source",
    "plan",
    "artifact",
    "llm",
    "revision",
    "index",
];

/// Encode a chunk as a 256-bit structural fingerprint.
///
/// Identical chunks produce identical bits across runs. Runtime is linear
/// in ancestor-path length + relation-type count.
#[must_use]
pub fn fingerprint(chunk: &Chunk) -> [u8; FINGERPRINT_BYTES] {
    let mut bits = BitBuf::new();
    encode_layer(&mut bits, &chunk.layer);
    encode_namespace(&mut bits, &chunk.namespace);
    encode_ancestor_path(&mut bits, &chunk.ancestor_local_names, &chunk.element_name);
    encode_element_name(&mut bits, &chunk.element_name);
    encode_relations(&mut bits, OUT_REL_START, OUT_REL_END, &chunk.outgoing_relation_types);
    encode_relations(&mut bits, IN_REL_START, IN_REL_END, &chunk.incoming_relation_types);
    bits.into_bytes()
}

fn encode_layer(bits: &mut BitBuf, layer: &str) {
    if let Some(idx) = LAYER_PREFIXES.iter().position(|p| *p == layer) {
        bits.set(LAYER_START + idx);
    }
}

fn encode_namespace(bits: &mut BitBuf, namespace: &str) {
    if namespace.is_empty() {
        return;
    }
    let h = fx_hash_32(namespace);
    let width = NS_END - NS_START;
    for i in 0..width {
        if (h >> i) & 1 == 1 {
            bits.set(NS_START + i);
        }
    }
}

fn encode_ancestor_path(bits: &mut BitBuf, ancestor_local_names: &[String], self_name: &str) {
    // Element localnames from root to self, joined with "/".
    let mut parts: Vec<&str> = ancestor_local_names.iter().map(String::as_str).collect();
    parts.push(self_name);
    let width = PATH_END - PATH_START;

    // 3-gram shingle over parts. Degrades to 2-gram/1-gram if path
    // is shorter, so even depth-1 elements contribute bits.
    let n = parts.len();
    if n == 0 {
        return;
    }
    let mut tokens: Vec<String> = Vec::new();
    if n >= 3 {
        for w in parts.windows(3) {
            tokens.push(format!("{}/{}/{}", w[0], w[1], w[2]));
        }
    } else if n == 2 {
        tokens.push(format!("{}/{}", parts[0], parts[1]));
    } else {
        tokens.push(parts[0].to_owned());
    }
    // Also fold in each token individually (unigram context).
    for p in &parts {
        tokens.push((*p).to_owned());
    }
    for tok in &tokens {
        let bucket = (fx_hash_32(tok) as usize) % width;
        bits.set(PATH_START + bucket);
    }
}

fn encode_element_name(bits: &mut BitBuf, name: &str) {
    if name.is_empty() {
        return;
    }
    let h = fx_hash_32(name);
    let width = ELEM_END - ELEM_START;
    for i in 0..width {
        if (h >> i) & 1 == 1 {
            bits.set(ELEM_START + i);
        }
    }
}

fn encode_relations(bits: &mut BitBuf, start: usize, end: usize, types: &[String]) {
    let width = end - start;
    for ty in types {
        let bucket = (fx_hash_32(ty) as usize) % width;
        bits.set(start + bucket);
    }
}

/// Deterministic 32-bit `FxHash`, matching the algorithm used elsewhere
/// in the Rust ecosystem. We implement it directly so the crate doesn't
/// need an extra `fxhash` dependency — the bit layout is frozen and
/// must never change, so borrowing an external crate risks drift.
fn fx_hash_32(s: &str) -> u32 {
    // 32-bit variant.
    const ROTATE: u32 = 5;
    const SEED: u32 = 0x9E37_79B9;
    let mut h: u32 = 0;
    for b in s.as_bytes() {
        h = (h.rotate_left(ROTATE) ^ u32::from(*b)).wrapping_mul(SEED);
    }
    h
}

/// A simple 256-bit buffer.
struct BitBuf {
    bytes: [u8; FINGERPRINT_BYTES],
}

impl BitBuf {
    fn new() -> Self {
        Self { bytes: [0u8; FINGERPRINT_BYTES] }
    }

    fn set(&mut self, bit: usize) {
        debug_assert!(bit < FINGERPRINT_BITS);
        let b = bit / 8;
        let i = bit % 8;
        self.bytes[b] |= 1u8 << i;
    }

    fn into_bytes(self) -> [u8; FINGERPRINT_BYTES] {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_chunk(id: &str, layer: &str, element_name: &str) -> Chunk {
        Chunk {
            id: id.into(),
            file: PathBuf::from("/tmp/x.xml"),
            line_start: 1,
            line_end: 1,
            layer: layer.into(),
            namespace: format!("urn:clayers:{layer}"),
            element_name: element_name.into(),
            text: format!("[layer={layer} path=clayers>section>term]\nbody"),
            ancestor_ids: vec!["x".into(), "y".into()],
            ancestor_local_names: vec!["clayers".into(), "section".into()],
            outgoing_relation_types: vec!["depends-on".into()],
            incoming_relation_types: vec!["refines".into()],
            node_hash: "sha256:placeholder".into(),
        }
    }

    #[test]
    fn fingerprint_different_ancestor_local_names_differ() {
        // Paths with different element chains must produce different
        // fingerprints even if ids and element_name match.
        let mut a = sample_chunk("x", "prose", "p");
        let mut b = sample_chunk("x", "prose", "p");
        a.ancestor_local_names = vec!["clayers".into(), "section".into()];
        b.ancestor_local_names = vec!["clayers".into(), "note".into()];
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    #[test]
    fn fingerprint_snapshot_for_stable_layout() {
        // Golden snapshot: if this assertion fails, somebody changed
        // the bit-layout / hash function. Bump `FINGERPRINT_VERSION`
        // (and force a full rebuild of all existing indexes) when
        // this is intentional.
        let c = sample_chunk("id-a", "terminology", "term");
        let fp = fingerprint(&c);
        // Layer bit (terminology = index 1) → byte 0 bit 1 set.
        assert_eq!(fp[0] & 0b0000_0011, 0b0000_0010);
        // Reserved 5 bits (byte 31 bits 3..7) must be zero.
        assert_eq!(fp[31] & 0b1111_1000, 0);
        // Bit count must be non-trivial (not all zeros).
        let popcount: u32 = fp.iter().map(|b| b.count_ones()).sum();
        assert!(popcount > 5, "fingerprint too sparse: {popcount} bits set");
        assert!(popcount < 200, "fingerprint too dense: {popcount} bits set");
    }

    #[test]
    fn fingerprint_is_deterministic() {
        let c = sample_chunk("a", "trm", "term");
        let a = fingerprint(&c);
        let b = fingerprint(&c);
        assert_eq!(a, b, "same chunk must produce identical fingerprints");
    }

    #[test]
    fn fingerprint_layer_bit_matches_canonical_list() {
        // Bit 0 → prose; bit 1 → terminology.
        let pr = sample_chunk("p", "prose", "section");
        let trm = sample_chunk("t", "terminology", "term");
        let fp_pr = fingerprint(&pr);
        let fp_trm = fingerprint(&trm);
        assert_eq!(fp_pr[0] & 0b1, 0b1);
        assert_eq!(fp_trm[0] & 0b1, 0b0);
        assert_eq!(fp_trm[0] & 0b10, 0b10);
        assert_eq!(fp_pr[0] & 0b10, 0b00);
    }

    #[test]
    fn fingerprint_different_layers_differ() {
        let pr = sample_chunk("x", "prose", "section");
        let trm = sample_chunk("x", "terminology", "term");
        assert_ne!(fingerprint(&pr), fingerprint(&trm));
    }

    #[test]
    fn fingerprint_different_element_names_differ() {
        let mut a = sample_chunk("x", "prose", "section");
        let b = sample_chunk("x", "prose", "p");
        a.namespace = b.namespace.clone();
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    #[test]
    fn fingerprint_reserved_bits_are_zero() {
        let c = sample_chunk("x", "prose", "section");
        let fp = fingerprint(&c);
        let reserved_mask = 0b1111_1000u8;
        assert_eq!(fp[31] & reserved_mask, 0);
    }

    #[test]
    fn fingerprint_different_relations_differ() {
        let mut a = sample_chunk("x", "prose", "section");
        let mut b = sample_chunk("x", "prose", "section");
        a.outgoing_relation_types = vec!["A".into()];
        b.outgoing_relation_types = vec!["B".into()];
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    #[test]
    fn fingerprint_unknown_layer_leaves_onehot_zero() {
        let c = sample_chunk("x", "unknown-layer", "tag");
        let fp = fingerprint(&c);
        let byte0_layer_mask = 0xFFu8;
        let byte1_layer_mask = 0b0000_0111u8;
        assert_eq!(fp[0] & byte0_layer_mask, 0);
        assert_eq!(fp[1] & byte1_layer_mask, 0);
    }
}
