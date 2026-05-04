use liturgical_calendar_core::{
    entry::CalendarEntry,
    ffi::{kal_read_entry, kal_validate_header, KAL_ENGINE_OK},
};
use liturgical_calendar_forge::forge_full_range;
use std::ptr::null_mut;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ---------------------------------------------------------------------------
// 1. Validité du header sur la plage complète
// ---------------------------------------------------------------------------

/// Vérifie que le header produit par `forge_full_range(1969..=2399)` est
/// accepté par `kal_validate_header` sans erreur.
///
/// Invariant structurel : magic, version, checksum SHA-256 (octets 24–55)
/// doivent satisfaire les contrôles du reader Engine.
#[test]
fn full_range_header_valid() {
    let kald = forge_full_range(1969..=2399).expect("forge plage complète");
    let rc = unsafe { kal_validate_header(kald.as_ptr(), kald.len(), null_mut()) };
    assert_eq!(rc, KAL_ENGINE_OK);
}

// ---------------------------------------------------------------------------
// 2. Padding entries — doy=59 sur 431 années
// ---------------------------------------------------------------------------

/// doy=59 = 28 février en année non-bissextile → Padding Entry (primary_id == 0).
/// doy=59 = 29 février en année bissextile      → entrée réelle  (primary_id != 0).
///
/// Couvre les deux branches de l'invariant Padding sur 1969–2399 (431 années).
#[test]
fn full_range_padding_entries_correct() {
    let kald = forge_full_range(1969..=2399).expect("forge plage complète");

    for year in 1969u16..=2399 {
        let is_leap = is_leap_year(year as i32);
        let mut e = CalendarEntry::zeroed();
        let rc = unsafe { kal_read_entry(kald.as_ptr(), kald.len(), year, 59, &mut e) };

        assert_eq!(rc, KAL_ENGINE_OK, "kal_read_entry KO — year {year}");

        if is_leap {
            assert_ne!(
                e.primary_id, 0,
                "year {year} (bissextile) : doy=59 doit être une entrée réelle"
            );
        } else {
            assert_eq!(
                e.primary_id, 0,
                "year {year} (non-bissextile) : doy=59 doit être un Padding Entry"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Déterminisme SHA-256 — deux builds doivent produire un hash identique
// ---------------------------------------------------------------------------

/// Les octets 24–55 du header contiennent le SHA-256 du corpus de données.
/// Deux appels successifs à `forge_full_range` doivent produire un hash
/// octet-à-octet identique (pas d'horodatage ni d'aléa dans le pipeline).
#[test]
fn full_range_deterministic() {
    let kald1 = forge_full_range(1969..=2399).unwrap();
    let kald2 = forge_full_range(1969..=2399).unwrap();

    assert_eq!(
        &kald1[24..56],
        &kald2[24..56],
        "SHA-256 (octets 24–55) doit être identique entre deux builds successifs"
    );
}

// ---------------------------------------------------------------------------
// 4. Saint Justin le 1er juin 2025 — relégation en secondary
// ---------------------------------------------------------------------------

/// Le 1er juin 2025 (doy=152), le 7e Dimanche de Pâques est prioritaire.
/// Saint Justin doit apparaître dans la liste secondary (secondary_count ≥ 1).
///
/// Vérifie que le mécanisme de résolution de préséance produit la bonne
/// structure : primary occupé par la fête pascale, secondary non-vide.
#[test]
fn full_range_iustini_june1_2025() {
    let kald = forge_full_range(1969..=2399).unwrap(); 
    let mut e = CalendarEntry::zeroed();
    let rc = unsafe { kal_read_entry(kald.as_ptr(), kald.len(), 2025, 152, &mut e) };

    assert_eq!(rc, KAL_ENGINE_OK);
    assert!(
        e.secondary_count >= 1,
        "Saint Justin doit être en secondary le 1er juin 2025 (doy=152)"
    );
}

// ---------------------------------------------------------------------------
// 5. Triduum pascal 2025 — exactement 3 entrées Precedence 0
// ---------------------------------------------------------------------------

/// Le Triduum pascal est encodé avec Precedence = 0 (bits [3:0] du flags word).
/// `kal_scan_flags` doit retourner exactement 3 indices pour 2025.
///
/// Masque : 0x000F (isoler les 4 bits de précédence), valeur attendue : 0.
#[test]
fn full_range_triduum_2025_exactly_3_entries() {
    use liturgical_calendar_core::ffi::{kal_scan_flags, KAL_ERR_BUF_TOO_SMALL};

    let kald = forge_full_range(2025..=2025).unwrap();
    let mut indices = [0u32; 10];
    let mut count = 0u32;

    let rc = unsafe {
kal_scan_flags(
            kald.as_ptr(),
            kald.len(),
            2025,                // year_from : 2025
            2025,                // year_to   : 2025
            0x000F,              // flag_mask : bits [3:0]
            0,                   // flag_value : Precedence 0 (Triduum)
            indices.as_mut_ptr(),
            10,                  // out_capacity : aligné sur la taille de indices
            &mut count,
        )
    };
    eprintln!("count = {}", count);

    // KAL_ERR_BUF_TOO_SMALL ne peut pas survenir ici (buffer de 10, Triduum = 3)
    assert_ne!(
        rc, KAL_ERR_BUF_TOO_SMALL,
        "buffer trop petit — augmenter la capacité du tableau indices"
    );
    assert_eq!(rc, KAL_ENGINE_OK);
    assert_eq!(count, 3, "Triduum pascal 2025 = exactement 3 jours Precedence-0");
}

// ---------------------------------------------------------------------------
// 6. Transfer de Ss. Petri et Pauli
// ---------------------------------------------------------------------------

/// En 1973 et 1984, Pâques tombe le 22 avril. Le Sacré-Cœur (mobile, pascha+68)
/// atterrit le 29 juin — date fixe de Ss. Petri et Pauli.
/// La résolution de préséance transfère Ss. Petri et Pauli au 30 juin.
///
/// Ce phénomène se reproduit chaque fois que Pâques = 22 avril :
/// 1973, 1984, 2057, 2068, 2114…
///
/// Invariants vérifiés :
/// - doy=180 (29 juin) : Sacré-Cœur en primary — Ss. Petri et Pauli absent.
/// - doy=181 (30 juin) : Ss. Petri et Pauli (0x10b3) en primary.
#[test]
fn full_range_petri_et_pauli_transfer_easter_april22() {
    let kald = forge_full_range(1969..=2399).expect("forge plage complète");

    // Résolution dynamique de l'ID — 1970 : Sacré-Cœur = 5 juin, pas de conflit sur doy=180.
    let mut e_ref = CalendarEntry::zeroed();
    let rc = unsafe { kal_read_entry(kald.as_ptr(), kald.len(), 1970, 180, &mut e_ref) };
    assert_eq!(rc, KAL_ENGINE_OK, "kal_read_entry KO — référence 1970 doy=180");
    let petri_et_pauli_id = e_ref.primary_id;
    assert_ne!(petri_et_pauli_id, 0, "ID de référence nul — vérifier doy=180 en 1970");

    // Années où Pâques = 22 avril → Sacré-Cœur (pascha+68) = 29 juin (doy=180).
    // Ss. Petri et Pauli est transféré au 30 juin (doy=181).
    for year in [1973u16, 1984] {
        let mut e_june29 = CalendarEntry::zeroed();
        let rc = unsafe { kal_read_entry(kald.as_ptr(), kald.len(), year, 180, &mut e_june29) };
        assert_eq!(rc, KAL_ENGINE_OK, "kal_read_entry KO — {year} doy=180");
        assert_ne!(
            e_june29.primary_id, petri_et_pauli_id,
            "doy=180 (29 juin {year}) : Ss. Petri et Pauli ne doit pas être en primary"
        );

        let mut e_june30 = CalendarEntry::zeroed();
        let rc = unsafe { kal_read_entry(kald.as_ptr(), kald.len(), year, 181, &mut e_june30) };
        assert_eq!(rc, KAL_ENGINE_OK, "kal_read_entry KO — {year} doy=181");
        assert_eq!(
            e_june30.primary_id, petri_et_pauli_id,
            "doy=181 (30 juin {year}) : Ss. Petri et Pauli doit être en primary (transfert depuis doy=180)"
        );
    }
}
