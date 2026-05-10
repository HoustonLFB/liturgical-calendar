//! Étape 5 — Day Materialization : Timeline v5 + vespers lookahead.
//!
//! Changements v4 → v5 :
//!   - `CalendarEntry`    → `TimelineEntry` (stride 8 conservé)
//!   - `flags[13:0]`      → migrent dans `FeastEntry.flags` (Feast Registry)
//!   - `flags[14:15]`     → migrent dans `TimelineEntry.occurrence_flags` bits [0:1]
//!   - `primary_id`       → `primary_index` (registry_index 1-based, 0 = Padding)
//!   - `secondary_index`  → `secondary_offset`
//!   - `PoolBuilder`      : stocke des `registry_index` (1-based), pas des `feast_id`
//!   - `vespers_lookahead_pass` : lit la préséance depuis le Feast Registry

#![allow(missing_docs)]

use std::collections::BTreeMap;

use liturgical_calendar_core::{
    entry::{FeastEntry, TimelineEntry},
    types::{Color, LiturgicalPeriod, Nature},
};

use crate::{
    canonicalization::{is_leap_year, SeasonBoundaries},
    error::ForgeError,
    resolution::ResolvedCalendar,
};

// ─── FeastRegistryBuilder ─────────────────────────────────────────────────────

/// Constructeur du Feast Registry AOT.
///
/// Maintient `feast_id → registry_index` (1-based).
/// Index `0` = sentinel Padding, jamais assigné.
/// Déterministe : le premier `get_or_insert` pour un `feast_id` donné fixe son index.
pub(crate) struct FeastRegistryBuilder {
    index:       BTreeMap<u16, u16>,
    pub entries: Vec<FeastEntry>,
}

impl FeastRegistryBuilder {
    pub fn new() -> Self {
        Self { index: BTreeMap::new(), entries: Vec::new() }
    }

    /// Retourne le `registry_index` existant ou insère et assigne le suivant (1-based).
    pub fn get_or_insert(&mut self, feast_id: u16, flags: u16) -> Result<u16, ForgeError> {
        if let Some(&idx) = self.index.get(&feast_id) {
            return Ok(idx);
        }
        if self.entries.len() >= u16::MAX as usize {
            return Err(ForgeError::RegistryOverflow);
        }
        let idx = self.entries.len() as u16 + 1;
        self.index.insert(feast_id, idx);
        self.entries.push(FeastEntry { feast_id, flags });
        Ok(idx)
    }

    pub fn index_of(&self, feast_id: u16) -> Option<u16> {
        self.index.get(&feast_id).copied()
    }

    pub fn registry_count(&self) -> u32 {
        self.entries.len() as u32
    }

    /// Slice pour `vespers_lookahead_pass` et la sérialisation.
    /// `entries[registry_index − 1]` = FeastEntry pour `registry_index` donné.
    pub fn as_slice(&self) -> &[FeastEntry] {
        &self.entries
    }
}

// ─── PoolBuilder ─────────────────────────────────────────────────────────────

/// Secondary Pool — stocke des `registry_index` (1-based) dédupliqués.
pub(crate) struct PoolBuilder {
    index: BTreeMap<Vec<u16>, u16>,
    pub data: Vec<u16>,
}

impl PoolBuilder {
    pub fn new() -> Self {
        Self { index: BTreeMap::new(), data: Vec::new() }
    }

    /// Insère une séquence de `registry_index` et retourne son offset (en u16).
    pub fn insert(&mut self, mut indices: Vec<u16>) -> Result<u16, ForgeError> {
        indices.sort_unstable();

        if let Some(&existing) = self.index.get(&indices) {
            return Ok(existing);
        }
        if self.data.len() + indices.len() > u16::MAX as usize {
            return Err(ForgeError::SecondaryPoolOverflow {
                pool_len:     self.data.len() as u32,
                max_capacity: u16::MAX as u32,
            });
        }
        let offset = self.data.len() as u16;
        self.index.insert(indices.clone(), offset);
        self.data.extend_from_slice(&indices);
        Ok(offset)
    }
}

// ─── encode_feast_flags ───────────────────────────────────────────────────────

/// Encode les invariants d'une fête dans `FeastEntry.flags`.
///
/// - bits [3:0]   → `precedence`
/// - bits [7:4]   → `color`
/// - bits [10:8]  → `liturgical_period`
/// - bits [13:11] → `nature`
/// - bit  [14]    → `has_vigil_mass`
/// - bit  [15]    → réservé, nul
pub(crate) fn encode_feast_flags(
    precedence:        u8,
    color:             Color,
    liturgical_period: LiturgicalPeriod,
    nature:            Nature,
    has_vigil_mass:    bool,
) -> u16 {
    (precedence as u16)
        | ((color as u16)             << 4)
        | ((liturgical_period as u16) << 8)
        | ((nature as u16)            << 11)
        | ((has_vigil_mass as u16)    << 14)
}

// ─── build_feast_registry (Pass 1) ────────────────────────────────────────────

/// Pass 1 — collecte tous les `feast_id` du corpus et construit le Feast Registry.
///
/// Prend des références — `ResolvedCalendar` n'a pas besoin d'implémenter `Clone`.
///
/// Hypothèse : `ResolvedDay.secondary_feasts` expose les mêmes champs que `day.primary`
/// (`feast_id`, `precedence`, `color`, `nature`, `has_vigil_mass`).
pub(crate) fn build_feast_registry<'a>(
    all_inputs: &[(&'a ResolvedCalendar, &'a SeasonBoundaries)],
) -> Result<FeastRegistryBuilder, ForgeError> {
    let mut builder = FeastRegistryBuilder::new();

    for &(resolved, season_boundaries) in all_inputs {
        let is_leap = is_leap_year(resolved.year);

        for (&doy, day) in &resolved.days {
            if !is_leap && doy == 59 {
                continue;
            }

            let period = season_boundaries.period_of(doy);

            let primary_flags = encode_feast_flags(
                day.primary.precedence,
                day.primary.color,
                period,
                day.primary.nature,
                day.primary.has_vigil_mass,
            );
            builder.get_or_insert(day.primary.feast_id, primary_flags)?;

            for secondary in &day.secondary_feasts {
                let sec_flags = encode_feast_flags(
                    secondary.precedence,
                    secondary.color,
                    period,
                    secondary.nature,
                    secondary.has_vigil_mass,
                );
                builder.get_or_insert(secondary.feast_id, sec_flags)?;
            }
        }
    }

    Ok(builder)
}

// ─── generate_year (Pass 2) ───────────────────────────────────────────────────

/// Pass 2 — génère les 366 `TimelineEntry` pour une année résolue.
///
/// Requiert que `feast_registry` soit complet (Pass 1 terminée) :
/// tout `feast_id` rencontré doit y être présent.
/// `occurrence_flags` laissés à `0` — `vespers_lookahead_pass` les calcule ensuite.
pub(crate) fn generate_year(
    resolved:             &ResolvedCalendar,
    pool:                 &mut PoolBuilder,
    _season_boundaries:   &SeasonBoundaries,
    feast_registry:       &FeastRegistryBuilder,
) -> Result<[TimelineEntry; 366], ForgeError> {
    let year    = resolved.year;
    let is_leap = is_leap_year(year);

    let mut entries = [TimelineEntry::zeroed(); 366];

    for doy in 0u16..=365u16 {
        if !is_leap && doy == 59 {
            continue;
        }

        let day = match resolved.days.get(&doy) {
            Some(d) => d,
            None    => continue,
        };

        let secondary_count = day.secondary_feasts.len();
        if secondary_count > u8::MAX as usize {
            return Err(ForgeError::SecondaryCountOverflow { doy, year, count: secondary_count });
        }

        let primary_index = feast_registry
            .index_of(day.primary.feast_id)
            .ok_or(ForgeError::FeastNotInRegistry { feast_id: day.primary.feast_id, year, doy })?;

        let (secondary_offset, sc) = if secondary_count > 0 {
            let indices: Vec<u16> = day.secondary_feasts
                .iter()
                .map(|f| {
                    feast_registry
                        .index_of(f.feast_id)
                        .ok_or(ForgeError::FeastNotInRegistry { feast_id: f.feast_id, year, doy })
                })
                .collect::<Result<_, _>>()?;
            let offset = pool.insert(indices)?;
            (offset, secondary_count as u8)
        } else {
            (0u16, 0u8)
        };

        entries[doy as usize] = TimelineEntry {
            primary_index,
            secondary_offset,
            occurrence_flags: 0,
            secondary_count:  sc,
            _reserved:        0,
        };
    }

    if !is_leap {
        let e = &entries[59];
        if e.primary_index != 0 || e.secondary_count != 0 {
            return Err(ForgeError::PaddingEntryMissing { year, doy: 59 });
        }
    }

    Ok(entries)
}

// ─── vespers_lookahead_pass ────────────────────────────────────────────────────

/// Passe vespérale — calcule `occurrence_flags` bits [0:1] de chaque `TimelineEntry`.
///
/// Opère APRÈS `generate_year`. Lit la préséance et `has_vigil_mass` depuis `feast_registry`.
///
/// Règles (inchangées depuis v4) :
///   - bit 0 (HAS_VESPERAE_I) : si `tomorrow_prec ≤ 3 || tomorrow_prec == 5`
///     ET `today_prec ≥ tomorrow_prec || today_prec == 0`.
///   - bit 1 (HAS_VIGILIA)    : reporté depuis `FeastEntry.has_vigil_mass()` de demain.
///
/// `next_year_jan1` : premier slot de l'année suivante.
/// `None` pour 2399 — bits conservés à 0.
pub(crate) fn vespers_lookahead_pass(
    entries:        &mut [TimelineEntry; 366],
    feast_registry: &[FeastEntry],
    next_year_jan1: Option<&TimelineEntry>,
) {
    #[inline]
    fn prec_of(e: &TimelineEntry, r: &[FeastEntry]) -> u8 {
        if e.primary_index == 0 { return 0; }
        r.get(e.primary_index as usize - 1)
            .map_or(0, |fe| (fe.flags & 0x0F) as u8)
    }

    #[inline]
    fn vigil_of(e: &TimelineEntry, r: &[FeastEntry]) -> bool {
        if e.primary_index == 0 { return false; }
        r.get(e.primary_index as usize - 1)
            .is_some_and(|fe| fe.flags & (1 << 14) != 0)
    }

    for doy in 0u16..=365u16 {
        let (tomorrow_prec, tomorrow_has_vigil) = if doy < 365 {
            let t = &entries[doy as usize + 1];
            (prec_of(t, feast_registry), vigil_of(t, feast_registry))
        } else {
            match next_year_jan1 {
                Some(e) => (prec_of(e, feast_registry), vigil_of(e, feast_registry)),
                None    => continue,
            }
        };

        let has_first_vespers = tomorrow_prec <= 3 || tomorrow_prec == 5;
        if !has_first_vespers { continue; }

        let today_prec = prec_of(&entries[doy as usize], feast_registry);
        if today_prec != 0 && today_prec < tomorrow_prec { continue; }

        let mut occ = entries[doy as usize].occurrence_flags;
        occ |= 1 << 0;
        if tomorrow_has_vigil { occ |= 1 << 1; }
        entries[doy as usize].occurrence_flags = occ;
    }
}
