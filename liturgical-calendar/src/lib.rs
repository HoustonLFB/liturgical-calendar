//! Façade du calendrier liturgique catholique (Novus Ordo, 1969–2399).
//!
//! Ce crate est le point d'entrée unique pour les utilisateurs Rust.
//! Il ré-exporte l'intégralité de la surface publique de
//! [`liturgical-calendar-core`](https://docs.rs/liturgical-calendar-core).
//!
//! # Accès à une entrée
//!
//! ```rust,no_run
//! use liturgical_calendar::{kal_read_entry, CalendarEntry, KAL_ENGINE_OK};
//!
//! let kald: Vec<u8> = std::fs::read("romanus_universale.kald").unwrap();
//! let mut entry = CalendarEntry::zeroed();
//!
//! let rc = unsafe { kal_read_entry(kald.as_ptr(), kald.len(), 2025, 109, &mut entry) };
//! assert_eq!(rc, KAL_ENGINE_OK);
//!
//! if !entry.is_padding() {
//!     let nature     = entry.nature().unwrap();
//!     let precedence = entry.precedence().unwrap();
//!     let color      = entry.color().unwrap();
//! }
//! ```
//!
//! # Labels i18n
//!
//! ```rust,no_run
//! use liturgical_calendar::lits_provider::LitsProvider;
//!
//! let lits: Vec<u8> = std::fs::read("romanus_universale_la.lits").unwrap();
//! let provider = LitsProvider::new(&lits).unwrap();
//!
//! // Définition de l'invariant de recherche (ID de la fête)
//! let feast_id = 109;
//!
//! if let Some(entry) = provider.get(feast_id, 2025) {
//!     println!("{}", entry.label);
//!     if let Some(ann) = entry.annotation {
//!         println!("{}", ann);
//!     }
//! }
//! ```
//!
//! # Intégration C / FFI
//!
//! La génération du header C s'effectue depuis [`liturgical-calendar-core`](https://crates.io/crates/liturgical-calendar-core) :
//!
//! ```bash
//! cargo build -p liturgical-calendar-core --features gen-headers
//! ```
//!
//! Les intégrateurs FFI ciblent directement le core — la façade n'intervient pas dans ce flux.
//!
//! ```text
//! YAML corpus
//!   → kal-forge (build-time)
//!     → romanus_universale.kald
//!     → romanus_universale_la.lits
//!       → liturgical_calendar::kal_read_entry  (runtime, O(1))
//!       → liturgical_calendar::lits_provider::LitsProvider
//! ```

#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]

// ── Ré-exportations ───────────────────────────────────────────────────────────

pub use liturgical_calendar_core::{
    // Structure principale
    CalendarEntry,
    Header,

    // FFI — lecture
    kal_read_entry,
    kal_validate_header,

    // FFI — codes de retour
    KAL_ENGINE_OK,
    KAL_ERR_BUF_TOO_SMALL,
    KAL_ERR_CHECKSUM,
    KAL_ERR_FILE_SIZE,
    KAL_ERR_INDEX_OOB,
    KAL_ERR_MAGIC,
    KAL_ERR_NULL_PTR,
    KAL_ERR_POOL_OOB,
    KAL_ERR_RESERVED,
    KAL_ERR_SCHEMA,
    KAL_ERR_VERSION,

    // Types de domaine
    types::{Color, DomainError, LiturgicalPeriod, Nature, Precedence},

    // Provider i18n
    lits_provider,
};
