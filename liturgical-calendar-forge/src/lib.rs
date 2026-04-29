#![allow(missing_docs)] // activé en Jalon 3

pub mod error;
pub mod registry;
pub mod lock;
pub mod variant_lock;
pub mod parsing;
pub mod canonicalization;
pub mod resolution;
pub mod materialization;
pub mod packing;
pub mod i18n;
pub(crate) mod lits_writer;

// ── Exports publics ───────────────────────────────────────────────────────────

pub use variant_lock::VariantRegistryLock;
pub use error::ForgeError;
pub use registry::FeastRegistry;
pub use parsing::{ingest_corpus, parse_feast_from_yaml, allocate_feast_ids};
pub(crate) use packing::build_kald;
pub use canonicalization::{
    CanonicalizedYear, PreResolvedTransfers, AnchorTable,
    canonicalize_year, compute_easter, build_anchor_table,
    resolve_adventus, resolve_nativitas, resolve_epiphania,
    resolve_tempus_ordinarium, MONTH_STARTS, is_leap_year,
};

// ── Imports internes ──────────────────────────────────────────────────────────

use std::path::Path;
use materialization::{generate_year, vespers_lookahead_pass, PoolBuilder};
use packing::write_kald;
use resolution::resolve_year;

// ── Types publics ─────────────────────────────────────────────────────────────

/// Paramètres i18n pour la production des fichiers `.lits` compagnons.
///
/// Si `None` est passé à `compile`, aucun `.lits` n'est produit (comportement
/// Session B inchangé — `.kald` seul).
pub struct I18nConfig<'a> {
    /// Chemin vers `corpus/{rite}/i18n/` — racine de l'arborescence des dictionnaires.
    pub i18n_root: &'a Path,
    /// Répertoire de sortie pour les fichiers `.lits` produits.
    /// Un fichier `{lang}.lits` y est écrit par langue découverte.
    pub lits_dir: &'a Path,
}

// ── Pipeline de compilation ───────────────────────────────────────────────────

/// Compile un corpus YAML en fichier `.kald` pour la plage 1969–2399.
/// Si `i18n` est fourni, produit également un `.lits` par langue compilée.
///
/// # Pipeline
///
/// - Étape 1    — Allocation des FeastIDs stables (lock)
/// - Étape 1bis — i18n Resolution (si `i18n` fourni)
/// - Étapes 3–5 — Canonicalization → Conflict Resolution → Materialization
/// - Étape 6    — Binary Packing `.kald` puis `.lits` (si `i18n` fourni)
///
/// # Retour
///
/// SHA-256 `[u8; 32]` du `.kald` produit (`checksum[..8]` = Build ID).
/// Le même Build ID est inscrit dans le header de chaque `.lits`.
pub fn compile(
    registry:    FeastRegistry,
    output:      &Path,
    variant_id:  u16,
    i18n:        Option<I18nConfig<'_>>,
    lock_path:   &Path,
) -> Result<[u8; 32], ForgeError> {
    // ── Étape 1 — Allocation d'IDs stables ───────────────────────────────────
    let mut lock  = crate::lock::FeastRegistryLock::load(lock_path)?;
    let feast_ids = allocate_feast_ids(&registry, &mut lock, lock_path)?;

    // ── Validation post-merge : class obligatoire sur toute fête active ──────
    for feast in registry.iter() {
        if feast.class.is_none() {
            return Err(ForgeError::Parse(
                crate::error::ParseError::MissingClassAfterMerge {
                    slug: feast.slug.clone(),
                }
            ));
        }
    }

    // ── Étape 1bis — i18n Resolution ─────────────────────────────────────────
    let i18n_artifacts = match &i18n {
        Some(cfg) => {
            let mut store = i18n::DictStore::new();
            let langs     = i18n::discover_and_load_i18n(cfg.i18n_root, &mut store)?;
            i18n::remap_default_from_keys(&mut store, &registry);
            i18n::propagate_labels(&mut store, &registry);
            i18n::validate_i18n(&registry, &store)?;
            Some((store, langs))
        }
        None => None,
    };

    // ── Étapes 3–5 — Canonicalization → Resolution → Materialization ─────────
    let mut pool = PoolBuilder::new();
    let mut all_entries: Vec<[liturgical_calendar_core::CalendarEntry; 366]> =
        Vec::with_capacity(431);

    for year in 1969u16..=2399 {
        let canon    = canonicalize_year(year, &registry)?;
        let sb       = canon.season_boundaries.clone();
        let resolved = resolve_year(canon, &registry, &feast_ids)?;
        let entries  = generate_year(resolved, &mut pool, &sb)?;
        all_entries.push(entries);
    }

    // Vespers lookahead — accès simultané i et i+1 via split_at_mut.
    for i in 0..all_entries.len() {
        let (left, right) = all_entries.split_at_mut(i + 1);
        let next_jan1     = right.first().map(|e| &e[0]);
        vespers_lookahead_pass(&mut left[i], next_jan1);
    }

    // ── Étape 6 — Binary Packing `.kald` ─────────────────────────────────────
    let kald_checksum = write_kald(output, all_entries, pool, variant_id)?;

    // ── Étape 6 — Binary Packing `.lits` (une par langue) ────────────────────
    if let (Some(cfg), Some((store, langs))) = (&i18n, i18n_artifacts) {
        let lang_refs: Vec<&str> = langs.iter().map(String::as_str).collect();
        let table = i18n::build_label_table(&registry, &store, &feast_ids, &lang_refs);
        for lang in &lang_refs {
            let lits_path = cfg.lits_dir.join(format!("{}.lits", lang));
            crate::lits_writer::write_lits(&lits_path, &table, lang, &kald_checksum)?;
        }
    }

    Ok(kald_checksum)
}

// ── Helper de test ────────────────────────────────────────────────────────────

/// Compile le corpus complet en buffer binaire `.kald` sans I/O disque (hors lock).
/// Toujours 431 années (1969–2399) — layout AOT invariant.
pub fn forge_full_range(_range: std::ops::RangeInclusive<u16>) -> Result<Vec<u8>, ForgeError> {
    static LOCK_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    let corpus_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../corpus/romanus")
        .canonicalize()
        .map_err(ForgeError::Io)?;

    eprintln!("[DEBUG] corpus_path = {}", corpus_path.display());
    eprintln!("[DEBUG] corpus_path exists = {}", corpus_path.exists());

    let registry = ingest_corpus(&corpus_path)?;
    eprintln!("[DEBUG] {} fêtes chargées", registry.len());

    let feast_ids = {
        let _guard   = LOCK_MUTEX.lock().unwrap();
        let lock_path = std::env::temp_dir().join("liturgical_forge_test.lock");
        let mut lock  = crate::lock::FeastRegistryLock::load(&lock_path)?;
        allocate_feast_ids(&registry, &mut lock, &lock_path)?
    }; // _guard dropped ici — section critique minimale

    let mut pool = PoolBuilder::new();
    let mut all_entries = Vec::with_capacity(431);

    for year in 1969u16..=2399 {
        let canon    = canonicalize_year(year, &registry)?;
        let sb       = canon.season_boundaries.clone();
        let resolved = resolve_year(canon, &registry, &feast_ids)?;
        let entries  = generate_year(resolved, &mut pool, &sb)?;
        all_entries.push(entries);
    }

    for i in 0..all_entries.len() {
        let (left, right) = all_entries.split_at_mut(i + 1);
        let next_jan1     = right.first().map(|e| &e[0]);
        vespers_lookahead_pass(&mut left[i], next_jan1);
    }

    let (_checksum, bytes) = build_kald(all_entries, pool, 0)?;
    Ok(bytes)
}
