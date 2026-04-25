//! Étape 4 — Conflict Resolution : pipeline 5 passes.
//!
//! Hypothèses sur les types Session A (ajuster si divergence) :
//!   FeastRegistry::feasts       : BTreeMap<String, FeastDef>
//!   FeastDef::feast_id          : u16
//!   FeastDef::active_version_for(year) -> Option<&FeastVersionDef>
//!   FeastVersionDef::precedence : Precedence
//!   FeastVersionDef::nature     : Nature
//!   FeastVersionDef::color      : Color
//!   FeastVersionDef::has_vigil_mass : bool
//!   FeastVersionDef::date       : Option<(u8, u8)>   — (month, day)
//!   FeastVersionDef::mobile     : Option<MobileDef>
//!   FeastVersionDef::transfers  : Vec<TransferRule>
//!   MobileDef::anchor           : String
//!   MobileDef::offset           : i32
//!   MobileDef::ordinal          : Option<u8>         — tempus_ordinarium uniquement
//!   TransferRule::collides      : String
//!   TransferRule::target        : TransferTarget
//!   TransferTarget              : Offset(u32) | Date{m,d} | Mobile{anchor,offset}
//!   CanonicalizedYear::pre_resolved_transfers : BTreeMap<(String,String), u16>
//!   SeasonBoundaries::period_of(&self, doy: u16) -> LiturgicalPeriod

#![allow(missing_docs)]

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

// --- Imports Core (Types binaires optimisés) ---
use liturgical_calendar_core::{
    Color as CoreColor,
    Nature as CoreNature,
    LiturgicalPeriod as CorePeriod,
};

// --- Import Registry (Contrat YAML / Ingestion) ---
// On aliase pour ne pas percuter CorePeriod
use crate::registry::LiturgicalPeriod as RegistryPeriod;

use crate::{
    canonicalization::{
        is_leap_year, resolve_tempus_ordinarium, CanonicalizedYear, MONTH_STARTS,
    },
    error::ForgeError,
    registry::{
        FeastDef, FeastRegistry, Scope,
        Temporality as RegistryTemporality, // qualification obligatoire — conflit de nom
        TransferTarget,
    },
};

// ─── Nouvel outil de conversion ───────────────────────────────────────────────

/// Transforme la période du Registre vers le type binaire du Core.
/// Garantit le respect du layout de 3 bits défini dans l'ADR.
pub(crate) fn period_to_core(p: &RegistryPeriod) -> CorePeriod {
    match p {
        RegistryPeriod::TempusOrdinarium    => CorePeriod::TempusOrdinarium,
        RegistryPeriod::TempusAdventus      => CorePeriod::TempusAdventus,
        RegistryPeriod::TempusNativitatis   => CorePeriod::TempusNativitatis,
        RegistryPeriod::TempusQuadragesimae => CorePeriod::TempusQuadragesimae,
        RegistryPeriod::TriduumPaschale     => CorePeriod::TriduumPaschale,
        RegistryPeriod::TempusPaschale      => CorePeriod::TempusPaschale,
        RegistryPeriod::DiesSancti          => CorePeriod::DiesSancti,
    }
}

// ─── FeastIdMap ───────────────────────────────────────────────────────────────

/// `slug → FeastID` alloué. INV-FORGE-2 : BTreeMap.
/// Calculé une fois avant la boucle annuelle dans `compile()`.
pub(crate) type FeastIdMap = BTreeMap<String, u16>;

// ─── Conversions registry → Core ─────────────────────────────────────────────
// Nécessaires car registry::Color / registry::Nature ≠ liturgical_calendar_core::Color/Nature.

fn color_to_core(c: &crate::registry::Color) -> CoreColor {
    use crate::registry::Color as R;
    match c {
        R::Albus     => CoreColor::Albus,
        R::Rubeus    => CoreColor::Rubeus,
        R::Viridis   => CoreColor::Viridis,
        R::Violaceus => CoreColor::Violaceus,
        R::Rosaceus  => CoreColor::Rosaceus,
        R::Niger     => CoreColor::Niger,
        // Aureus : réservé dans Core v2.0 (valeur 6 non définie).
        // Fallback Albus — à revoir si Core expose Color::Aureus.
        R::Aureus    => CoreColor::Albus,
    }
}

fn nature_to_core(n: &crate::registry::Nature) -> CoreNature {
    use crate::registry::Nature as R;
    match n {
        R::Sollemnitas  => CoreNature::Sollemnitas,
        R::Festum       => CoreNature::Festum,
        R::Dominica     => CoreNature::Dominica,
        R::Memoria      => CoreNature::Memoria,
        R::Commemoratio => CoreNature::Commemoratio,
        R::Feria        => CoreNature::Feria,
    }
}

// ─── Cycle — utilisé dans elect pour temporal_primary ────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cycle {
    Temporal  = 0,
    Sanctoral = 1,
}

// ─── ResolutionKey — ADR-038 ──────────────────────────────────────────────────
//
// `sort_weight` = (internal_precedence << 2) | class_weight — 6 bits effectifs.
// Valeur plus faible = priorité plus haute.
//
// Bits [5:2] : préséance interne 0-based (0=Triduum, 12=Memoria ad libitum).
// Bits [1:0] : classe (0=Lord, 1=Virgin, 2=Saint, 3=Proper).
//
// `feast_id` : tiebreaker numérique pur — comparaison scalaire, zéro
// cache-miss, déterminisme garanti sans comparaison de chaînes.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ResolutionKey {
    pub sort_weight: u8,   // (precedence << 2) | class
    pub feast_id:    u16,  // tiebreaker final
}

// ─── PlacedFeast ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlacedFeast {
    pub slug:           String,
    pub feast_id:       u16,
    pub scope_bits:     u8,   // 0=Universal 1=National 2=Diocesan
    pub precedence:     u8,
    pub class:          u8,   // ADR-038 : 0=Lord 1=Virgin 2=Saint 3=Proper
    pub nature:         CoreNature,
    pub color:          CoreColor,
    pub period:         Option<CorePeriod>,
    pub has_vigil_mass: bool,
    pub cycle:          Cycle, // utilisé dans elect pour temporal_primary
}

impl PlacedFeast {
    #[inline]
    fn key(&self) -> ResolutionKey {
        ResolutionKey {
            sort_weight: (self.precedence << 2) | self.class,
            feast_id:    self.feast_id,
        }
    }
}

impl PartialOrd for PlacedFeast {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for PlacedFeast {
    fn cmp(&self, other: &Self) -> Ordering { self.key().cmp(&other.key()) }
}

// ─── ResolvedDay / ResolvedCalendar ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct ResolvedDay {
    pub primary:          PlacedFeast,
    pub secondary_feasts: Vec<PlacedFeast>,
}

pub(crate) struct ResolvedCalendar {
    pub year: u16,
    pub days: BTreeMap<u16, ResolvedDay>,
}

// ─── TransferQueue ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Eq, PartialEq)]
struct TransferEntry {
    doy_current: u16,
    feast_id:    u16,
    depth:       u8,
    feast:       PlacedFeast,
}

impl Ord for TransferEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.doy_current, self.feast_id).cmp(&(other.doy_current, other.feast_id))
    }
}
impl PartialOrd for TransferEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

const MAX_TRANSFER_DEPTH: u8 = 7;

struct TransferQueue {
    pending: BTreeSet<TransferEntry>,
}

impl TransferQueue {
    fn new() -> Self { Self { pending: BTreeSet::new() } }

    fn enqueue(
        &mut self, doy_src: u16, feast: PlacedFeast, depth: u8, year: u16,
    ) -> Result<(), ForgeError> {
        if depth > MAX_TRANSFER_DEPTH {
            return Err(ForgeError::TransferFailed {
                slug:       feast.slug.clone(),
                origin_doy: doy_src.saturating_sub(depth as u16),
                blocked_at: doy_src,
                year,
            });
        }
        self.pending.insert(TransferEntry {
            doy_current: doy_src,
            feast_id: feast.feast_id,
            depth,
            feast,
        });
        Ok(())
    }

    fn pop_first(&mut self) -> Option<TransferEntry> {
        let e = self.pending.iter().next()?.clone();
        self.pending.remove(&e);
        Some(e)
    }

    fn is_empty(&self) -> bool { self.pending.is_empty() }
}

// ─── Déclassement saisonnier — §3.4 ─────────────────────────────────────────

pub(crate) fn should_demote_to_commemoratio(
    feast: &PlacedFeast,
    period: CorePeriod,
) -> bool {
    // Toutes les mémoires (générales=9, propres=10, facultatives=11)
    // perdent leur caractère prescriptif en période privilegiée.
    feast.precedence >= 9
        && matches!(
            period,
            CorePeriod::TempusQuadragesimae
                | CorePeriod::TempusAdventus
                | CorePeriod::TriduumPaschale
                | CorePeriod::DiesSancti
        )
}

// ─── DOY depuis FeastDef.temporality ─────────────────────────────────────────
// Temporality est sur FeastDef, pas sur FeastHistoryEntry.

fn feast_doy(feast_def: &FeastDef, anchors: &BTreeMap<String, u16>) -> Option<u16> {
    match feast_def.temporality.as_ref()? {  // None temporality → None DOY → skip
        RegistryTemporality::Fixed { month, day } => {
            Some(MONTH_STARTS[*month as usize - 1] + *day as u16 - 1)
        }
        RegistryTemporality::Mobile { anchor, offset } => {
            let anchor_doy = anchors.get(anchor.as_str())?;
            let doy = *anchor_doy as i32 + offset;
            (0..=365).contains(&doy).then_some(doy as u16)
        }
        RegistryTemporality::Ordinal { ordinal } => {
            let adventus = *anchors.get("adventus")?;
            Some(resolve_tempus_ordinarium(adventus, *ordinal))
        }
    }
}

/// Déduit le Cycle depuis la temporalité de la fête.
/// Appelé uniquement si `feast_doy` a retourné `Some` — temporality garantie `Some`.
fn feast_cycle(feast_def: &FeastDef) -> Cycle {
    match feast_def.temporality.as_ref().expect("temporality absente après merge") {
        RegistryTemporality::Fixed { .. }         => Cycle::Sanctoral,
        RegistryTemporality::Mobile { .. }
        | RegistryTemporality::Ordinal { .. }     => Cycle::Temporal,
    }
}

// ─── Élection canonique ───────────────────────────────────────────────────────

fn elect(
    mut candidates: Vec<PlacedFeast>,
    period:         CorePeriod,
) -> (PlacedFeast, Vec<PlacedFeast>, Vec<PlacedFeast>) {
    candidates.sort_unstable_by_key(|a| a.key());

    let primary = candidates.remove(0);
    let temporal_primary = primary.cycle == Cycle::Temporal;

    let mut secondary_feasts = Vec::new();
    let mut to_transfer      = Vec::new();

    for feast in candidates {
        if (temporal_primary && should_demote_to_commemoratio(&feast, period))
            || feast.precedence >= 6
        {
            // secondary : FestaBMVEtSanctorumGenerales (6) et rangs inférieurs
            secondary_feasts.push(feast);
        } else if feast.nature == CoreNature::Dominica || feast.nature == CoreNature::Feria {
            // Dominica et Feria vaincues : absorption silencieuse — jamais transférées.
            // Invariant liturgique : un dimanche ordinaire ne se reporte pas.
        } else if feast.precedence <= 7 {
            // transferable : FestaPropria (7) et rangs supérieurs (0-5 atteignent ici)
            to_transfer.push(feast);
        }
        // else : supprimé silencieusement.
    }

    secondary_feasts.sort_unstable_by_key(|f| f.feast_id); // INV-FORGE-4
    (primary, secondary_feasts, to_transfer)
}

// ─── resolve_year ─────────────────────────────────────────────────────────────

pub(crate) fn resolve_year(
    canonicalized: CanonicalizedYear,
    registry:      &FeastRegistry,
    feast_ids:     &FeastIdMap,
) -> Result<ResolvedCalendar, ForgeError> {
    let year    = canonicalized.year;
    let is_leap = is_leap_year(year);

    // ── PASSE 1 ───────────────────────────────────────────────────────────────

    let mut slots: BTreeMap<u16, Vec<PlacedFeast>> = BTreeMap::new();

    for feast_def in registry.iter() {
        let version = match feast_def.active_version_for(year) {
            Some(v) => v,
            None    => continue,
        };

        let doy = match feast_doy(feast_def, &canonicalized.anchors) {
            Some(d) => d,
            None    => continue,
        };

        if !is_leap && doy == 59 { continue; }

        let feast_id = match feast_ids.get(&feast_def.slug) {
            Some(&id) => id,
            None      => continue,
        };

        let scope_bits: u8 = match &feast_def.scope {
            Scope::Universal   => 0,
            Scope::National(_) => 1,
            Scope::Diocesan(_) => 2,
        };

        let cycle = feast_cycle(feast_def);

        let precedence = version.precedence
            .ok_or_else(|| {
                eprintln!("ERREUR: Champ 'precedence' manquant pour le slug: {}", feast_def.slug);
                ForgeError::MissingResolvedField { feast_id, year, doy, field: "precedence" }
            })?;

        let nature_val = version.nature.as_ref()
            .ok_or_else(|| {
                eprintln!("ERREUR: Champ 'nature' manquant pour le slug: {}", feast_def.slug);
                ForgeError::MissingResolvedField { feast_id, year, doy, field: "nature" }
            })?;

        let color_val = version.color.as_ref()
            .ok_or_else(|| {
                eprintln!("ERREUR: Champ 'color' manquant pour le slug: {}", feast_def.slug);
                ForgeError::MissingResolvedField { feast_id, year, doy, field: "color" }
            })?;

        // ADR-038 : class obligatoire après merge — None = corpus universale incomplet.
        let class: u8 = feast_def.class
            .ok_or_else(|| {
                eprintln!("ERREUR: Champ 'class' manquant pour le slug: {}", feast_def.slug);
                ForgeError::MissingResolvedField { feast_id, year, doy, field: "class" }
            })? as u8;

        slots.entry(doy).or_default().push(PlacedFeast {
            slug:           feast_def.slug.clone(),
            feast_id,
            scope_bits,
            precedence,
            class,
            nature:         nature_to_core(nature_val),
            color:          color_to_core(color_val),
            period:         version.period.as_ref().map(period_to_core),
            has_vigil_mass: version.has_vigil_mass,
            cycle,
        });
    }

    // ── PASSE 2 ───────────────────────────────────────────────────────────────

    for (&doy, candidates) in slots.iter_mut() {
        // V7a : TriduumSacrum (0) et SollemnitatesMaiores (1) — uniques par construction.
        {
            let very_high: Vec<_> = candidates.iter().filter(|f| f.precedence <= 1).collect();
            if very_high.len() >= 2 {
                return Err(ForgeError::SolemnityCollision {
                    slug_a:     very_high[0].slug.clone(),
                    slug_b:     very_high[1].slug.clone(),
                    precedence: very_high[0].precedence,
                    doy, year,
                });
            }
        }

        // V7b : SollemnitatesGenerales (2) et SollemnitatesPropria (3).
        // Erreur uniquement si même scope ET même classe — deux classes différentes
        // sont résolvables par elect via sort_weight (ADR-038).
        {
            let solemn: Vec<_> = candidates.iter()
                .filter(|f| f.precedence >= 2 && f.precedence <= 3)
                .collect();
            for i in 0..solemn.len() {
                for j in (i + 1)..solemn.len() {
                    if solemn[i].scope_bits == solemn[j].scope_bits
                        && solemn[i].class == solemn[j].class
                    {
                        return Err(ForgeError::SolemnityCollision {
                            slug_a:     solemn[i].slug.clone(),
                            slug_b:     solemn[j].slug.clone(),
                            precedence: solemn[i].precedence,
                            doy, year,
                        });
                    }
                }
            }
        }

        // §3.1 — scope le plus local prime pour les Solennités.
        // Solennités susceptibles de conflit inter-scope : rangs 2 et 3.
        if candidates.iter().filter(|f| f.precedence <= 3).count() >= 2 {
            let max_scope = candidates.iter()
                .filter(|f| f.precedence <= 3)
                .map(|f| f.scope_bits)
                .max()
                .unwrap_or(0);
            candidates.retain(|f| !(f.precedence <= 3 && f.scope_bits < max_scope));
        }
    }

    // ── PASSE 3 ───────────────────────────────────────────────────────────────

    let mut resolved_days:      BTreeMap<u16, ResolvedDay>      = BTreeMap::new();
    let mut transfer_queue                                      = TransferQueue::new();
    let mut pending_inserts:    BTreeMap<u16, Vec<PlacedFeast>> = BTreeMap::new();
    let mut retrograde_inserts: Vec<(u16, PlacedFeast)>         = Vec::new();

    for doy in 0u16..=365u16 {
        let mut candidates: Vec<PlacedFeast> = slots.remove(&doy).unwrap_or_default();
        if let Some(fwd) = pending_inserts.remove(&doy) {
            candidates.extend(fwd);
        }
        if candidates.is_empty() { continue; }

        let period = canonicalized.season_boundaries.period_of(doy);
        let (primary, secondary_feasts, to_transfer) = elect(candidates, period);

        for feast in to_transfer {
            let active_rule = registry.get(&feast.slug)
                .and_then(|def| def.active_version_for(year))
                .and_then(|ver| ver.transfers.iter().find(|t| t.collides == primary.slug));

            if let Some(rule) = active_rule {
                let pre_key = (feast.slug.clone(), rule.collides.clone());
                if let Some(&doy_dst) = canonicalized.pre_resolved_transfers.get(&pre_key) {
                    if doy_dst <= doy {
                        retrograde_inserts.push((doy_dst, feast));
                    } else {
                        pending_inserts.entry(doy_dst).or_default().push(feast);
                    }
                    continue;
                }

                let doy_dst: u16 = match &rule.target {
                    TransferTarget::Offset(n) => doy + *n as u16,
                    TransferTarget::Date { month, day } => {
                        MONTH_STARTS[*month as usize - 1] + *day as u16 - 1
                    }
                    TransferTarget::Mobile { .. } => {
                        // Mobile sans PreResolved — bug Étape 3 ; fallback générique.
                        transfer_queue.enqueue(doy, feast, 0, year)?;
                        continue;
                    }
                };

                if doy_dst <= doy {
                    retrograde_inserts.push((doy_dst, feast));
                } else {
                    pending_inserts.entry(doy_dst).or_default().push(feast);
                }
            } else {
                transfer_queue.enqueue(doy, feast, 0, year)?;
            }
        }

        resolved_days.insert(doy, ResolvedDay { primary, secondary_feasts });
    }

    // Inserts rétrogrades — tri par doy_dst pour déterminisme.
    retrograde_inserts.sort_unstable_by_key(|(d, _)| *d);
    for (doy_dst, feast) in retrograde_inserts {
        let period = canonicalized.season_boundaries.period_of(doy_dst);
        if let Some(day) = resolved_days.get_mut(&doy_dst) {
            let mut all = vec![day.primary.clone(), feast];
            all.extend(day.secondary_feasts.clone());
            let (new_primary, new_secondary, _) = elect(all, period);
            day.primary          = new_primary;
            day.secondary_feasts = new_secondary;
        } else {
            resolved_days.insert(doy_dst, ResolvedDay {
                primary: feast, secondary_feasts: Vec::new(),
            });
        }
    }

    // ── PASSE 4 ───────────────────────────────────────────────────────────────

    while let Some(entry) = transfer_queue.pop_first() {
        let TransferEntry { doy_current, feast, depth, .. } = entry;
        let mut placed = false;

        let window_end = (doy_current + 7).min(365);
        for doy_dst in (doy_current + 1)..=window_end {
            let slot_free = match resolved_days.get(&doy_dst) {
                Some(day) => day.primary.precedence > feast.precedence,
                None      => true,
            };
            if !slot_free { continue; }

            let period = canonicalized.season_boundaries.period_of(doy_dst);
            let mut all = vec![feast.clone()];
            if let Some(existing) = resolved_days.remove(&doy_dst) {
                all.push(existing.primary);
                all.extend(existing.secondary_feasts);
            }
            let (new_primary, new_secondary, displaced) = elect(all, period);
            for d in displaced {
                transfer_queue.enqueue(doy_dst, d, depth + 1, year)?;
            }
            resolved_days.insert(doy_dst, ResolvedDay {
                primary: new_primary, secondary_feasts: new_secondary,
            });
            placed = true;
            break;
        }

        if !placed {
            return Err(ForgeError::TransferFailed {
                slug:       feast.slug.clone(),
                origin_doy: doy_current.saturating_sub(depth as u16),
                blocked_at: doy_current,
                year,
            });
        }
    }

    debug_assert!(transfer_queue.is_empty(), "TransferQueue non vide après Passe 4");

    // ── INTER-PASSE 4/5 — Feria generica pour doy=59 en année bissextile ─────────

    if is_leap && !resolved_days.contains_key(&59) {
        // Aucune fête ne tombe le 29 février — émettre une feria de substitution.
        // On cherche un slug feria générique dans le registry ; à défaut, ID réservé.
        let feria_id = feast_ids
            .get("feria_per_annum")
            .or_else(|| feast_ids.get("feria_generica"))
            .copied()
            .unwrap_or(0xFFFE); // ID de réserve si absent du corpus

        let period = canonicalized.season_boundaries.period_of(59);

        resolved_days.insert(59, ResolvedDay {
            primary: PlacedFeast {
                slug:           "feria_per_annum".into(),
                feast_id:       feria_id,
                scope_bits:     0,
                precedence:     13, // rang le plus bas
                class:          3,
                nature:         CoreNature::Feria,
                color:          CoreColor::Viridis,
                period:         Some(period),
                has_vigil_mass: false,
                cycle:          Cycle::Temporal,
            },
            secondary_feasts: Vec::new(),
        });
    }

    // ── PASSE 5 ───────────────────────────────────────────────────────────────

    for (&doy, day) in &resolved_days {
        if let Some(&expected_id) = feast_ids.get(&day.primary.slug) {
            if expected_id != day.primary.feast_id {
                return Err(ForgeError::FeastIDMutated {
                    slug:        day.primary.slug.clone(),
                    expected_id,
                    found_id:    day.primary.feast_id,
                    doy, year,
                });
            }
        }
    }

    Ok(ResolvedCalendar { year, days: resolved_days })
}
