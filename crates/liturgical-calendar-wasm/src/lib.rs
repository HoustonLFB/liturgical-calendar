#![no_std]

use core::ptr::addr_of;
use core::ptr::addr_of_mut;

use liturgical_calendar_core::{
    ffi::{kal_read_entry, kal_validate_header, KAL_ENGINE_OK},
    lits_provider::LitsProvider,
};

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

const KALD_CAP: usize = 2 * 1024 * 1024;
const LITS_CAP: usize = 512 * 1024;

static mut KALD_BUF: [u8; KALD_CAP] = [0u8; KALD_CAP];
static mut LITS_BUF: [u8; LITS_CAP] = [0u8; LITS_CAP];
static mut KALD_LEN: u32 = 0;
static mut LITS_LEN: u32 = 0;
static mut BRIDGE_STATE: u8 = 0;
static mut LABEL_PTR: u32 = 0;
static mut LABEL_LEN: u32 = 0;

pub const KAL_ERR_NOT_READY:          i32 = -20;
pub const KAL_ERR_BUF_OVERFLOW:       i32 = -21;
pub const KAL_ERR_BUILD_ID_MISMATCH:  i32 = -22;
pub const KAL_ERR_LITS_INVALID:       i32 = -23;

// ═════════════════════════════════════════════════════════════════════════════
// Phase 1 — allocation
// ═════════════════════════════════════════════════════════════════════════════

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_alloc_kald(len: u32) -> u32 {
    if len as usize > KALD_CAP {
        return 0;
    }
    // addr_of! produit un raw pointer sans créer de référence — pas d'UB.
    unsafe {
        *addr_of_mut!(KALD_LEN) = len;
        addr_of!(KALD_BUF) as u32
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_alloc_lits(len: u32) -> u32 {
    if len as usize > LITS_CAP {
        return 0;
    }
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
        if rc == KAL_ENGINE_OK {
            *addr_of_mut!(BRIDGE_STATE) = 1;
        }
        rc
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_commit_lits() -> i32 {
    unsafe {
        if *addr_of!(BRIDGE_STATE) < 1 {
            return KAL_ERR_NOT_READY;
        }
        if *addr_of!(LITS_LEN) < 20 {
            return KAL_ERR_LITS_INVALID;
        }

        // build_id : kald_checksum[..8] == lits_header[12..20].
        // Comparaison via raw pointers — pas de slice reference sur static mut.
        let kald_cs_ptr = (addr_of!(KALD_BUF) as *const u8).add(24);
        let lits_bi_ptr = (addr_of!(LITS_BUF) as *const u8).add(12);
        if core::slice::from_raw_parts(kald_cs_ptr, 8)
            != core::slice::from_raw_parts(lits_bi_ptr, 8)
        {
            return KAL_ERR_BUILD_ID_MISMATCH;
        }

        // Validation structurelle LitsProvider.
        let lits_slice = core::slice::from_raw_parts(
            addr_of!(LITS_BUF) as *const u8,
            *addr_of!(LITS_LEN) as usize,
        );
        match LitsProvider::new(lits_slice) {
            Ok(_) => {
                *addr_of_mut!(BRIDGE_STATE) = 2;
                KAL_ENGINE_OK
            }
            Err(_) => KAL_ERR_LITS_INVALID,
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Lecture entrée
// ═════════════════════════════════════════════════════════════════════════════

/// # Safety
/// `out_entry` doit être non-NULL et valide en écriture pour 8 octets.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kal_wasm_read_entry(
    year:      u16,
    doy:       u16,
    out_entry: *mut u8,
) -> i32 {
    unsafe {
        if *addr_of!(BRIDGE_STATE) != 2 {
            return KAL_ERR_NOT_READY;
        }
        kal_read_entry(
            addr_of!(KALD_BUF) as *const u8,
            *addr_of!(KALD_LEN) as usize,
            year,
            doy,
            out_entry as *mut _,
        )
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Résolution de label (pattern out-static)
// ═════════════════════════════════════════════════════════════════════════════

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_get_label(year: u16, doy: u16) -> i32 {
    unsafe {
        if *addr_of!(BRIDGE_STATE) != 2 {
            return KAL_ERR_NOT_READY;
        }

        let mut entry_buf = [0u8; 8];
        let rc = kal_read_entry(
            addr_of!(KALD_BUF) as *const u8,
            *addr_of!(KALD_LEN) as usize,
            year,
            doy,
            entry_buf.as_mut_ptr() as *mut _,
        );
        if rc != KAL_ENGINE_OK {
            return rc;
        }

        let primary_id = u16::from_le_bytes([entry_buf[0], entry_buf[1]]);
        if primary_id == 0 {
            return 0;
        }

        // Lifetime 'static : LITS_BUF est un static, durée de vie illimitée.
        let lits_slice: &'static [u8] = core::slice::from_raw_parts(
            addr_of!(LITS_BUF) as *const u8,
            *addr_of!(LITS_LEN) as usize,
        );

        let provider = match LitsProvider::new(lits_slice) {
            Ok(p) => p,
            Err(_) => return KAL_ERR_LITS_INVALID,
        };

        match provider.get(primary_id, year) {
            Some(entry) => {
                let bytes = entry.label.as_bytes();
                *addr_of_mut!(LABEL_PTR) = bytes.as_ptr() as u32;
                *addr_of_mut!(LABEL_LEN) = bytes.len() as u32;
                1
            }
            None => 0,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_label_ptr() -> u32 {
    unsafe { *addr_of!(LABEL_PTR) }
}

#[unsafe(no_mangle)]
pub extern "C" fn kal_wasm_label_len() -> u32 {
    unsafe { *addr_of!(LABEL_LEN) }
}
