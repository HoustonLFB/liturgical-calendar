#![no_std]

use core::ptr::{addr_of, addr_of_mut};

use liturgical_calendar_core::{
    ffi::{kal_read_entry, kal_read_secondary, kal_validate_header, KAL_ENGINE_OK},
    lits_provider::LitsProvider,
};

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

// ── Capacités ─────────────────────────────────────────────────────────────────

const KALD_CAP:      usize = 2 * 1024 * 1024;
const LITS_CAP:      usize = 512 * 1024;
const SECONDARY_CAP: usize = 32; // slots secondaires max par jour

// ── Buffers BSS ───────────────────────────────────────────────────────────────

static mut KALD_BUF:      [u8;  KALD_CAP]      = [0u8; KALD_CAP];
static mut LITS_BUF:      [u8;  LITS_CAP]      = [0u8; LITS_CAP];
static mut ENTRY_BUF:     [u8;  8]             = [0u8; 8];
static mut SECONDARY_BUF: [u16; SECONDARY_CAP] = [0u16; SECONDARY_CAP];

static mut KALD_LEN:    u32 = 0;
static mut LITS_LEN:    u32 = 0;
static mut BRIDGE_STATE: u8 = 0; // 0=Uninit 1=KaldLoaded 2=Ready

// ── Out-statics label + annotation ────────────────────────────────────────────

static mut LABEL_PTR:      u32 = 0;
static mut LABEL_LEN:      u32 = 0;
static mut ANNOTATION_PTR: u32 = 0; // 0 si annotation absente
static mut ANNOTATION_LEN: u32 = 0;

// ── Codes d'erreur bridge ─────────────────────────────────────────────────────

pub const KAL_ERR_NOT_READY:          i32 = -20;
pub const KAL_ERR_BUF_OVERFLOW:       i32 = -21;
pub const KAL_ERR_BUILD_ID_MISMATCH:  i32 = -22;
pub const KAL_ERR_LITS_INVALID:       i32 = -23;

// ── Helpers internes ──────────────────────────────────────────────────────────

/// Résout feast_id dans le LitsProvider et remplit les out-statics
/// LABEL_PTR/LEN + ANNOTATION_PTR/LEN. Retourne 1 si trouvé, 0 si absent.
///
/// # Safety
/// Doit être appelé depuis un bloc unsafe avec BRIDGE_STATE == 2.
unsafe fn resolve_by_id(feast_id: u16, year: u16) -> i32 {
    let lits_slice: &'static [u8] = unsafe {
        core::slice::from_raw_parts(
            addr_of!(LITS_BUF) as *const u8,
            *addr_of!(LITS_LEN) as usize,
        )
    };
    let provider = match LitsProvider::new(lits_slice) {
        Ok(p) => p,
        Err(_) => return KAL_ERR_LITS_INVALID,
    };
    unsafe {
        match provider.get(feast_id, year) {
            Some(entry) => {
                let lb = entry.label.as_bytes();
                *addr_of_mut!(LABEL_PTR) = lb.as_ptr() as u32;
                *addr_of_mut!(LABEL_LEN) = lb.len() as u32;
                match entry.annotation {
                    Some(ann) => {
                        let ab = ann.as_bytes();
                        *addr_of_mut!(ANNOTATION_PTR) = ab.as_ptr() as u32;
                        *addr_of_mut!(ANNOTATION_LEN) = ab.len() as u32;
                    }
                    None => {
                        *addr_of_mut!(ANNOTATION_PTR) = 0;
                        *addr_of_mut!(ANNOTATION_LEN) = 0;
                    }
                }
                1
            }
            None => {
                *addr_of_mut!(ANNOTATION_PTR) = 0;
                *addr_of_mut!(ANNOTATION_LEN) = 0;
                0
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Phase 1 — allocation
// ═════════════════════════════════════════════════════════════════════════════

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_alloc_kald(len: u32) -> u32 {
    if len as usize > KALD_CAP { return 0; }
    unsafe {
        *addr_of_mut!(KALD_LEN) = len;
        addr_of!(KALD_BUF) as u32
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_alloc_lits(len: u32) -> u32 {
    if len as usize > LITS_CAP { return 0; }
    unsafe {
        *addr_of_mut!(LITS_LEN) = len;
        addr_of!(LITS_BUF) as u32
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Phase 2 — commit
// ═════════════════════════════════════════════════════════════════════════════

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_commit_kald() -> i32 {
    unsafe {
        let rc = kal_validate_header(
            addr_of!(KALD_BUF) as *const u8,
            *addr_of!(KALD_LEN) as usize,
            core::ptr::null_mut(),
        );
        if rc == KAL_ENGINE_OK { *addr_of_mut!(BRIDGE_STATE) = 1; }
        rc
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_commit_lits() -> i32 {
    unsafe {
        if *addr_of!(BRIDGE_STATE) < 1 { return KAL_ERR_NOT_READY; }
        if *addr_of!(LITS_LEN) < 20    { return KAL_ERR_LITS_INVALID; }

        let kald_cs_ptr = (addr_of!(KALD_BUF) as *const u8).add(24);
        let lits_bi_ptr = (addr_of!(LITS_BUF) as *const u8).add(12);
        if core::slice::from_raw_parts(kald_cs_ptr, 8)
            != core::slice::from_raw_parts(lits_bi_ptr, 8)
        {
            return KAL_ERR_BUILD_ID_MISMATCH;
        }

        let lits_slice = core::slice::from_raw_parts(
            addr_of!(LITS_BUF) as *const u8,
            *addr_of!(LITS_LEN) as usize,
        );
        match LitsProvider::new(lits_slice) {
            Ok(_) => { *addr_of_mut!(BRIDGE_STATE) = 2; KAL_ENGINE_OK }
            Err(_) => KAL_ERR_LITS_INVALID,
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Lecture entrée — pattern out-static (ENTRY_BUF)
// ═════════════════════════════════════════════════════════════════════════════

/// Lit l'entrée `(year, doy)` dans `ENTRY_BUF`. Lire les champs via
/// `kal_wasm_entry_ptr()`. Retourne 0 (OK) ou code d'erreur négatif.
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_read_day(year: u16, doy: u16) -> i32 {
    unsafe {
        if *addr_of!(BRIDGE_STATE) != 2 { return KAL_ERR_NOT_READY; }
        kal_read_entry(
            addr_of!(KALD_BUF) as *const u8,
            *addr_of!(KALD_LEN) as usize,
            year,
            doy,
            addr_of_mut!(ENTRY_BUF) as *mut _,
        )
    }
}

/// Pointeur vers `ENTRY_BUF` (8 octets). Valide après `kal_wasm_read_day`.
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_entry_ptr() -> u32 {
    addr_of!(ENTRY_BUF) as u32
}

// ═════════════════════════════════════════════════════════════════════════════
// Fêtes secondaires — pattern out-static (SECONDARY_BUF)
// ═════════════════════════════════════════════════════════════════════════════

/// Remplit `SECONDARY_BUF` avec `count` IDs secondaires à partir de `index`.
/// Retourne le nombre d'IDs écrits (≥ 0) ou code d'erreur négatif.
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_read_secondary(index: u16, count: u8) -> i32 {
    unsafe {
        if *addr_of!(BRIDGE_STATE) != 2 { return KAL_ERR_NOT_READY; }
        kal_read_secondary(
            addr_of!(KALD_BUF) as *const u8,
            *addr_of!(KALD_LEN) as usize,
            index,
            count,
            addr_of_mut!(SECONDARY_BUF) as *mut u16,
            SECONDARY_CAP as u8,
        )
    }
}

/// Pointeur vers `SECONDARY_BUF` (u16 LE). Valide après `kal_wasm_read_secondary`.
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_secondary_ptr() -> u32 {
    addr_of!(SECONDARY_BUF) as u32
}

// ═════════════════════════════════════════════════════════════════════════════
// Résolution de label + annotation
// ═════════════════════════════════════════════════════════════════════════════

/// Résout label + annotation pour `(year, doy)` — lit d'abord l'entrée,
/// puis projette dans le LitsProvider.
///
/// Retourne 1 (trouvé), 0 (absent/Padding), < 0 (erreur).
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_get_label(year: u16, doy: u16) -> i32 {
    unsafe {
        if *addr_of!(BRIDGE_STATE) != 2 { return KAL_ERR_NOT_READY; }

        let rc = kal_read_entry(
            addr_of!(KALD_BUF) as *const u8,
            *addr_of!(KALD_LEN) as usize,
            year,
            doy,
            addr_of_mut!(ENTRY_BUF) as *mut _,
        );
        if rc != KAL_ENGINE_OK { return rc; }

        let primary_id = u16::from_le_bytes([ENTRY_BUF[0], ENTRY_BUF[1]]);
        if primary_id == 0 { return 0; }

        resolve_by_id(primary_id, year)
    }
}

/// Résout label + annotation directement par `feast_id` — sans passer par
/// `kal_read_entry`. Utilisé pour les fêtes secondaires.
///
/// Retourne 1 (trouvé), 0 (absent), < 0 (erreur).
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_get_label_by_id(feast_id: u16, year: u16) -> i32 {
    unsafe {
        if *addr_of!(BRIDGE_STATE) != 2 { return KAL_ERR_NOT_READY; }
        resolve_by_id(feast_id, year)
    }
}

// Accesseurs out-statics label
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_label_ptr() -> u32      { unsafe { *addr_of!(LABEL_PTR) } }
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_label_len() -> u32      { unsafe { *addr_of!(LABEL_LEN) } }

// Accesseurs out-statics annotation (ANNOTATION_LEN == 0 si absente)
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_annotation_ptr() -> u32 { unsafe { *addr_of!(ANNOTATION_PTR) } }
#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_annotation_len() -> u32 { unsafe { *addr_of!(ANNOTATION_LEN) } }

// ── Ancienne signature conservée pour compatibilité ──────────────────────────

/// # Safety
/// `out_entry` doit être non-NULL et valide en écriture pour 8 octets.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kal_wasm_read_entry(
    year:      u16,
    doy:       u16,
    out_entry: *mut u8,
) -> i32 {
    unsafe {
        if *addr_of!(BRIDGE_STATE) != 2 { return KAL_ERR_NOT_READY; }
        kal_read_entry(
            addr_of!(KALD_BUF) as *const u8,
            *addr_of!(KALD_LEN) as usize,
            year,
            doy,
            out_entry as *mut _,
        )
    }
}
