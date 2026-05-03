// benches/core_bench.rs
//
// Benchmarks Divan — liturgical-calendar-core, couverture FFI complète.
//
// Exécution :
//   cargo bench -p liturgical-calendar-core
//   cargo bench -p liturgical-calendar-core -- --filter kal_read_entry
//
// Toutes les fonctions benchmarkées sont `extern "C"` — on passe par la FFI
// exactement comme un consommateur C ou WASM.

use liturgical_calendar_core::{
    entry::{CalendarEntry, KALD_FORMAT_VERSION, LAYOUT_DISCRIMINANT},
    ffi::{
        kal_read_entry, kal_read_secondary, kal_scan_flags, kal_validate_header,
        KAL_ENGINE_OK,
    },
};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

// ── Helpers de fixtures ───────────────────────────────────────────────────────

/// Encode une `CalendarEntry` en 8 octets little-endian.
fn encode_entry(e: &CalendarEntry) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&e.primary_id.to_le_bytes());
    b[2..4].copy_from_slice(&e.secondary_index.to_le_bytes());
    b[4..6].copy_from_slice(&e.flags.to_le_bytes());
    b[6] = e.secondary_count;
    b[7] = e._reserved;
    b
}

/// Construit un buffer `.kald` valide.
///
/// `slots` : liste de `(idx, CalendarEntry)` à écrire dans le Data Body.
/// `pool`  : contenu brut du Secondary Pool (u16 LE concaténés).
fn build_kald(entry_count: u32, slots: &[(u32, CalendarEntry)], pool: &[u8]) -> Vec<u8> {
    let body_len = entry_count as usize * 8;
    let pool_offset = 64 + body_len;
    let total = pool_offset + pool.len();
    let mut buf = vec![0u8; total];

    for &(idx, ref entry) in slots {
        let offset = 64 + idx as usize * 8;
        buf[offset..offset + 8].copy_from_slice(&encode_entry(entry));
    }
    if !pool.is_empty() {
        buf[pool_offset..pool_offset + pool.len()].copy_from_slice(pool);
    }

    let mut hasher = Sha256::new();
    hasher.update(&buf[64..]);
    let checksum = hasher.finalize();

    buf[0..4].copy_from_slice(b"KALD");
    buf[4..6].copy_from_slice(&KALD_FORMAT_VERSION.to_le_bytes());
    buf[6..8].copy_from_slice(&0u16.to_le_bytes());          // variant_id = 0
    buf[8..10].copy_from_slice(&1969u16.to_le_bytes());      // epoch
    buf[10..12].copy_from_slice(&431u16.to_le_bytes());      // range
    buf[12..16].copy_from_slice(&entry_count.to_le_bytes());
    buf[16..20].copy_from_slice(&(pool_offset as u32).to_le_bytes()); // pool_offset
    buf[20..24].copy_from_slice(&(pool.len() as u32).to_le_bytes());  // pool_size
    buf[24..56].copy_from_slice(checksum.as_slice());
    buf[56..64].copy_from_slice(&LAYOUT_DISCRIMINANT.to_le_bytes());

    buf
}

// ── Fixtures statiques ────────────────────────────────────────────────────────

// 0 entrée — SHA-256 sur 0 octets : mesure le coût fixe du header parsing.
static KALD_EMPTY: OnceLock<Vec<u8>> = OnceLock::new();

// 431 ans × 366 slots = 157 746 entrées ≈ 1.26 Mo.
// Corpus synthétique : primary_id=1, flags cyclés sur 13 valeurs de Precedence.
// Utilisé par validate_header (SHA-256 sur ~1.26 Mo) et kal_scan_flags.
static KALD_FULL: OnceLock<Vec<u8>> = OnceLock::new();

// 1 entrée + Secondary Pool de 2 IDs — fixture minimale pour kal_read_secondary.
static KALD_WITH_SECONDARY: OnceLock<Vec<u8>> = OnceLock::new();

fn kald_empty() -> &'static [u8] {
    KALD_EMPTY.get_or_init(|| build_kald(0, &[], &[]))
}

fn kald_full() -> &'static [u8] {
    KALD_FULL.get_or_init(|| {
        const N: u32 = 431 * 366;
        let slots: Vec<(u32, CalendarEntry)> = (0..N)
            .map(|i| {
                (i, CalendarEntry {
                    primary_id: 1,
                    secondary_index: 0,
                    flags: (i % 13) as u16, // Precedence cyclée — simule densité réelle
                    secondary_count: 0,
                    _reserved: 0,
                })
            })
            .collect();
        build_kald(N, &slots, &[])
    })
}

fn kald_with_secondary() -> &'static [u8] {
    KALD_WITH_SECONDARY.get_or_init(|| {
        let entry = CalendarEntry {
            primary_id: 1,
            secondary_index: 0,
            flags: 0,
            secondary_count: 2,
            _reserved: 0,
        };
        let mut pool = [0u8; 4];
        pool[0..2].copy_from_slice(&42u16.to_le_bytes());
        pool[2..4].copy_from_slice(&99u16.to_le_bytes());
        build_kald(1, &[(0, entry)], &pool)
    })
}

// ── kal_validate_header ───────────────────────────────────────────────────────
//
// `empty` → coût fixe : parsing header + discriminant check (pas de SHA-256).
// `full`  → coût dominant : SHA-256 sur ~1.26 Mo de payload.

#[divan::bench]
fn kal_validate_header_empty() {
    let buf = kald_empty();
    let rc = unsafe {
        kal_validate_header(
            divan::black_box(buf.as_ptr()),
            divan::black_box(buf.len()),
            core::ptr::null_mut(),
        )
    };
    assert_eq!(rc, KAL_ENGINE_OK);
}

#[divan::bench]
fn kal_validate_header_full_431y() {
    let buf = kald_full();
    let rc = unsafe {
        kal_validate_header(
            divan::black_box(buf.as_ptr()),
            divan::black_box(buf.len()),
            core::ptr::null_mut(),
        )
    };
    assert_eq!(rc, KAL_ENGINE_OK);
}

// ── kal_read_entry ────────────────────────────────────────────────────────────
//
// Chemin O(1) : vérification de domaine + calcul d'offset + read_unaligned.
// Deux coordonnées pour éviter toute élimination par le compilateur.

#[divan::bench]
fn kal_read_entry_hot() {
    let buf = kald_full();
    let mut entry = CalendarEntry::zeroed();
    let rc = unsafe {
        kal_read_entry(
            divan::black_box(buf.as_ptr()),
            divan::black_box(buf.len()),
            divan::black_box(2025u16),
            divan::black_box(109u16), // Pâques 2025, doy 0-based
            &mut entry as *mut CalendarEntry,
        )
    };
    assert_eq!(rc, KAL_ENGINE_OK);
    divan::black_box(entry);
}

#[divan::bench]
fn kal_read_entry_last_slot() {
    let buf = kald_full();
    let mut entry = CalendarEntry::zeroed();
    let rc = unsafe {
        kal_read_entry(
            divan::black_box(buf.as_ptr()),
            divan::black_box(buf.len()),
            divan::black_box(2399u16),
            divan::black_box(364u16),
            &mut entry as *mut CalendarEntry,
        )
    };
    assert_eq!(rc, KAL_ENGINE_OK);
    divan::black_box(entry);
}

// ── kal_read_secondary ────────────────────────────────────────────────────────
//
// `zero_count` → retour immédiat après NULL check : mesure le overhead FFI pur.
// `count_2`    → lecture de 2 u16 depuis le Secondary Pool.

#[divan::bench]
fn kal_read_secondary_zero_count() {
    let buf = kald_with_secondary();
    let mut ids = [0u16; 4];
    let rc = unsafe {
        kal_read_secondary(
            divan::black_box(buf.as_ptr()),
            divan::black_box(buf.len()),
            divan::black_box(0u16),
            divan::black_box(0u8),
            ids.as_mut_ptr(),
            4u8,
        )
    };
    assert_eq!(rc, KAL_ENGINE_OK);
}

#[divan::bench]
fn kal_read_secondary_count_2() {
    let buf = kald_with_secondary();
    let mut ids = [0u16; 4];
    let rc = unsafe {
        kal_read_secondary(
            divan::black_box(buf.as_ptr()),
            divan::black_box(buf.len()),
            divan::black_box(0u16),
            divan::black_box(2u8),
            ids.as_mut_ptr(),
            4u8,
        )
    };
    assert_eq!(rc, KAL_ENGINE_OK);
    divan::black_box(ids);
}

// ── kal_scan_flags ────────────────────────────────────────────────────────────
//
// Seule fonction O(n) — scan linéaire, lecture séquentielle de 8 octets/entrée
// avec accès uniquement aux octets [0..1] (primary_id) et [4..5] (flags).
// Pattern DOD-friendly : stride constant, pas d'indirection.
//
// `single_year` → 366 entrées : coût minimal d'un scan annuel.
// `full_431y`   → 157 746 entrées : débit sur le corpus complet.

#[divan::bench]
fn kal_scan_flags_single_year() {
    let buf = kald_full();
    let mut indices = vec![0u32; 366];
    let mut count = 0u32;
    let rc = unsafe {
        kal_scan_flags(
            divan::black_box(buf.as_ptr()),
            divan::black_box(buf.len()),
            divan::black_box(2025u16),
            divan::black_box(2025u16),
            divan::black_box(0x000Fu16), // mask : Precedence bits [3:0]
            divan::black_box(0u16),       // value : TriduumSacrum (0)
            indices.as_mut_ptr(),
            indices.len() as u32,
            &mut count,
        )
    };
    assert_eq!(rc, KAL_ENGINE_OK);
    divan::black_box(count);
}

#[divan::bench]
fn kal_scan_flags_full_431y() {
    let buf = kald_full();
    let mut indices = vec![0u32; 431 * 366];
    let mut count = 0u32;
    let rc = unsafe {
        kal_scan_flags(
            divan::black_box(buf.as_ptr()),
            divan::black_box(buf.len()),
            divan::black_box(1969u16),
            divan::black_box(2399u16),
            divan::black_box(0x000Fu16),
            divan::black_box(0u16),
            indices.as_mut_ptr(),
            indices.len() as u32,
            &mut count,
        )
    };
    assert_eq!(rc, KAL_ENGINE_OK);
    divan::black_box(count);
}

fn main() {
    divan::main();
}
