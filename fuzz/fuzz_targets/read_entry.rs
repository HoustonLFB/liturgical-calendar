#![no_main]
//! Cible fuzz : `kal_read_entry` — kald v5.
//!
//! Layout du corpus d'entrée (minimum 4 octets) :
//!   [0..2]  → year  (u16 little-endian)
//!   [2..4]  → doy   (u16 little-endian, day-of-year 0–365)
//!   [4..]   → buf   (données `.kald` v5 arbitraires, potentiellement malformées)
//!
//! Invariant : aucun panic pour toute combinaison (year, doy, buf).
//! Les cas hors-plage (doy=400, buf vide, buf tronqué, header corrompu) doivent
//! retourner un code d'erreur, pas un UB.

use libfuzzer_sys::fuzz_target;
use liturgical_calendar_core::{
    entry::TimelineEntry,
    ffi::{kal_read_entry, kal_read_feast},
};

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }

    let year = u16::from_le_bytes([data[0], data[1]]);
    let doy  = u16::from_le_bytes([data[2], data[3]]);
    let buf  = &data[4..];

    // `TimelineEntry::zeroed()` garantit un état de départ déterministe.
    let mut entry = TimelineEntry::zeroed();

    let rc = unsafe { kal_read_entry(buf.as_ptr(), buf.len(), year, doy, &mut entry) };

    // Si kal_read_entry réussit et retourne un primary_index non-nul,
    // exercer kal_read_feast sur le même buffer — ne doit pas paniquer.
    if rc == 0 && entry.primary_index != 0 {
        use liturgical_calendar_core::entry::FeastEntry;
        let mut feast = FeastEntry::zeroed();
        let _ = unsafe {
            kal_read_feast(buf.as_ptr(), buf.len(), entry.primary_index, &mut feast)
        };
    }
});
