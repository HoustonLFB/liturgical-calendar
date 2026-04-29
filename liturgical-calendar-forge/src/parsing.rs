use serde::{Deserialize, Deserializer};
use std::collections::{BTreeSet, BTreeMap};
use std::path::Path;
use std::fs;

use crate::error::{ForgeError, ParseError, RegistryError};
use crate::registry::{
    Color, FeastDef, FeastHistoryEntry, FeastRegistry, LiturgicalClass, LiturgicalPeriod,
    Nature, Scope, Temporality, TransferDef, TransferTarget,
};
use crate::lock::FeastRegistryLock;

// ---------------------------------------------------------------------------
// Boundary Normalization : Precedence 1-based (YAML) → 0-based (interne)
// ---------------------------------------------------------------------------
//
// Le contrat amont (YAML / rédacteur humain) expose une plage 1–13 conforme
// à la Tabella dierum liturgicorum 1969 (rangs I à XIII).
// L'Engine et le pipeline interne utilisent 0–12 pour les opérations bitwise.
// La conversion s'effectue ici, au point d'entrée exact de la Forge (Serde),
// afin que nulle autre couche n'ait à connaître la convention YAML.

fn deserialize_precedence_opt<'de, D>(deserializer: D) -> Result<Option<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<u8>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(v) if matches!(v, 1..=13) => Ok(Some(v - 1)),
        Some(v) => Err(serde::de::Error::custom(format!(
            "precedence invalide : {} (attendu 1-13)",
            v
        ))),
    }
}

// ---------------------------------------------------------------------------
// Deserialize Collides
// ---------------------------------------------------------------------------

/// Accepte indifféremment `collides: slug` (String) ou `collides: [a, b]` (Vec).
fn deserialize_collides<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    match OneOrMany::deserialize(d)? {
        OneOrMany::One(s)   => Ok(vec![s]),
        OneOrMany::Many(v)  => Ok(v),
    }
}

// ---------------------------------------------------------------------------
// Structs de désérialisation YAML — deny_unknown_fields partout
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlFeast {
    version:   u32,
    category:  u8,
    id:        Option<u16>,
    date:      Option<YamlDate>,
    mobile:    Option<YamlMobile>,
    #[serde(default)]
    history:   Vec<YamlHistoryEntry>,
    /// Classe liturgique du sujet — ADR-038.
    /// Optionnel au parsing (deltas peuvent l'omettre).
    /// Validé présent après merge dans resolve_year (MissingResolvedField).
    #[serde(default)]
    class:     Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlDate { month: u8, day: u8 }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlMobile {
    anchor:  String,
    offset:  Option<i32>,
    ordinal: Option<u8>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlTransfer {
    #[serde(deserialize_with = "deserialize_collides")]
    collides: Vec<String>,
    offset:   Option<u32>,
    date:     Option<YamlDate>,
    mobile:   Option<YamlMobileDst>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlMobileDst {
    anchor: String,
    offset: i32,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct YamlHistoryEntry {
    from:           Option<u16>,
    to:             Option<u16>,
    #[serde(default, deserialize_with = "deserialize_precedence_opt")]
    precedence:     Option<u8>,
    nature:         Option<String>,
    color:          Option<String>,
    period:         Option<String>,
    has_vigil_mass: Option<bool>,
    transfers:      Option<Vec<YamlTransfer>>,  // scoped à cette tranche temporelle
}

// ---------------------------------------------------------------------------
// V6 — validation slug : [a-z][a-z0-9_]*
// ---------------------------------------------------------------------------

fn validate_slug(stem: &str) -> Result<(), ParseError> {
    let mut chars = stem.chars();
    match chars.next() {
        None | Some('0'..='9') | Some('_') => {
            return Err(ParseError::InvalidSlugSyntax(stem.to_string()))
        }
        Some(c) if !c.is_ascii_lowercase() => {
            return Err(ParseError::InvalidSlugSyntax(stem.to_string()))
        }
        _ => {}
    }
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' {
            return Err(ParseError::InvalidSlugSyntax(stem.to_string()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// V5 — parse Nature (avec hint sur valeurs informelles)
// ---------------------------------------------------------------------------

fn parse_nature(s: &str) -> Result<Nature, RegistryError> {
    match s {
        "sollemnitas"  => Ok(Nature::Sollemnitas),
        "festum"       => Ok(Nature::Festum),
        "dominica"     => Ok(Nature::Dominica),
        "memoria"      => Ok(Nature::Memoria),
        "commemoratio" => Ok(Nature::Commemoratio),
        "feria"        => Ok(Nature::Feria),
        other => {
            let hint = match other {
                "solemnity" | "solemnnitas" | "solemnitas" => " (hint: 'sollemnitas')",
                "feast"    => " (hint: 'festum')",
                "memorial" | "memory" => " (hint: 'memoria')",
                "commemoration" => " (hint: 'commemoratio')",
                _ => "",
            };
            Err(RegistryError::UnknownNatureString(format!("{}{}", other, hint)))
        }
    }
}

// ---------------------------------------------------------------------------
// Parse Color
// ---------------------------------------------------------------------------

fn parse_color(s: &str) -> Result<Color, RegistryError> {
    match s {
        "white" | "albus"              => Ok(Color::Albus),
        "red"   | "rubeus"             => Ok(Color::Rubeus),
        "green" | "viridis"            => Ok(Color::Viridis),
        "purple"| "violet"|"violaceus" => Ok(Color::Violaceus),
        "rose"  | "rosaceus"           => Ok(Color::Rosaceus),
        "black" | "niger"              => Ok(Color::Niger),
        "gold"  | "aureus"             => Ok(Color::Aureus),
        other => Err(RegistryError::UnknownColorString(other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Parse Liturgical Period
// ---------------------------------------------------------------------------

fn parse_period(s: &str) -> Result<LiturgicalPeriod, RegistryError> {
    match s {
        "tempus_ordinarium"    => Ok(LiturgicalPeriod::TempusOrdinarium),
        "tempus_adventus"      => Ok(LiturgicalPeriod::TempusAdventus),
        "tempus_nativitatis"   => Ok(LiturgicalPeriod::TempusNativitatis),
        "tempus_quadragesimae" => Ok(LiturgicalPeriod::TempusQuadragesimae),
        "triduum_paschale"     => Ok(LiturgicalPeriod::TriduumPaschale),
        "tempus_paschale"      => Ok(LiturgicalPeriod::TempusPaschale),
        "dies_sancti"          => Ok(LiturgicalPeriod::DiesSancti),
        other => Err(RegistryError::UnknownPeriodString(other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Parse LiturgicalClass — ADR-038
// ---------------------------------------------------------------------------

fn parse_class(s: &str) -> Result<LiturgicalClass, RegistryError> {
    match s {
        "lord"   => Ok(LiturgicalClass::Lord),
        "virgin" => Ok(LiturgicalClass::Virgin),
        "saint"  => Ok(LiturgicalClass::Saint),
        "proper" => Ok(LiturgicalClass::Proper),
        other => Err(RegistryError::UnknownClassString(other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// V3a — validation date (mois/jour cohérents ; Feb 29 admis statiquement)
// ---------------------------------------------------------------------------

fn validate_date(slug: &str, month: u8, day: u8) -> Result<(), ParseError> {
    if !(1..=12).contains(&month) {
        return Err(ParseError::InvalidDate { slug: slug.to_string(), month, day });
    }
    let max_day: u8 = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 29, // Feb 29 admis (l'année n'est pas connue à ce stade)
        _ => unreachable!(),
    };
    if !(1..=max_day).contains(&day) {
        return Err(ParseError::InvalidDate { slug: slug.to_string(), month, day });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// V-T5 — ancres primitives valides pour transfers.mobile
// ---------------------------------------------------------------------------

const PRIMITIVE_ANCHORS: &[&str] = &[
    "pascha", "adventus", "pentecostes", "nativitas", "epiphania",
];

// ---------------------------------------------------------------------------
// Parsing de la temporalité mobile (V4a + desucrage pentecostes)
// ---------------------------------------------------------------------------

fn parse_mobile_temporality(slug: &str, m: &YamlMobile) -> Result<Temporality, ForgeError> {
    if m.anchor == "tempus_ordinarium" {
        // V4a — tempus_ordinarium : offset interdit, ordinal obligatoire [1,34]
        if m.offset.is_some() {
            return Err(ParseError::OffsetOnOrdinalAnchor { slug: slug.to_string() }.into());
        }
        let ordinal = m.ordinal
            .ok_or_else(|| ParseError::MissingOrdinal { slug: slug.to_string() })?;
        if !(1..=34).contains(&ordinal) {
            return Err(ParseError::OrdinalOutOfRange { slug: slug.to_string(), ordinal }.into());
        }
        Ok(Temporality::Ordinal { ordinal })
    } else {
        // V4a — ancre ordinaire : ordinal interdit
        if m.ordinal.is_some() {
            return Err(ParseError::OrdinalOnNonOrdinalAnchor {
                slug:   slug.to_string(),
                anchor: m.anchor.clone(),
            }.into());
        }
        let offset = m.offset.unwrap_or(0);
        // Desugaring pentecostes → pascha + 49
        let (anchor, offset) = if m.anchor == "pentecostes" {
            ("pascha".to_string(), offset + 49)
        } else {
            (m.anchor.clone(), offset)
        };
        Ok(Temporality::Mobile { anchor, offset })
    }
}

// ---------------------------------------------------------------------------
// Parsing history (V2-Bis, V3b, V2d, V5, V-Natura-Memoria, V-Vigilia)
// ---------------------------------------------------------------------------

fn parse_history(slug: &str, entries: &[YamlHistoryEntry])
    -> Result<Vec<FeastHistoryEntry>, ForgeError>
{
    let mut result: Vec<FeastHistoryEntry> = Vec::with_capacity(entries.len());

    for entry in entries {
        let from = entry.from.unwrap_or(1969);
        let to   = entry.to.unwrap_or(2399);

        // V3b — plages temporelles
        if from < 1969 || to > 2399 || from > to {
            return Err(RegistryError::InvalidTemporalRange { from, to }.into());
        }

        let precedence = entry.precedence; // Option<u8> — transmis tel quel

        // V2-Bis — garanti par deserialize_precedence_opt (domaine 0–12 interne).
        // La debug_assert ne s'applique que si la valeur est présente.
        debug_assert!(
            precedence.is_none_or(|p| p <= 12),
            "Invariant V2-Bis violé après Serde"
        );

        let nature = entry.nature.as_deref().map(parse_nature).transpose()?;
        let color  = entry.color.as_deref().map(parse_color).transpose()?;
        let period = entry.period.as_deref().map(parse_period).transpose()?;
        let has_vigil_mass = entry.has_vigil_mass.unwrap_or(false);

        // V-Natura-Memoria — applicable uniquement si les deux champs sont présents.
        if let (Some(nat), Some(prec)) = (nature.as_ref(), precedence) {
            if *nat == Nature::Memoria && !matches!(prec, 9..=11) {
                return Err(ParseError::InvalidMemoriaPrecedence {
                    slug:             slug.to_string(),
                    from,
                    found_precedence: prec,
                }.into());
            }
        }

        // V-Vigilia — applicable uniquement si nature est présente.
        if has_vigil_mass && nature.as_ref().is_some_and(|n| *n != Nature::Sollemnitas) {
            return Err(ParseError::VigiliaNonSollemnitas {
                slug:   slug.to_string(),
                from,
                nature: entry.nature.clone().unwrap_or_default(),
            }.into());
        }

        // V-T* — transfers scoped à cette entrée history
        let transfers = entry.transfers
            .as_deref()
            .map(|ts| parse_transfers(slug, from, ts))
            .transpose()?
            .unwrap_or_default();

        result.push(FeastHistoryEntry {
            from,
            to,
            precedence,
            nature,
            color,
            period,
            has_vigil_mass,
            transfers,
        });
    }

    // V2d — chevauchement temporel : tri par `from`, détection intervalle
    check_temporal_overlap(slug, &result)?;

    Ok(result)
}

fn check_temporal_overlap(slug: &str, entries: &[FeastHistoryEntry])
    -> Result<(), ForgeError>
{
    let mut sorted: Vec<&FeastHistoryEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| e.from);
    for i in 1..sorted.len() {
        if sorted[i].from <= sorted[i - 1].to {
            eprintln!("[DEBUG] TemporalOverlap : slug={slug}  from[i]={} <= to[i-1]={}",
                sorted[i].from, sorted[i - 1].to);
            return Err(RegistryError::TemporalOverlap.into());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Parsing transfers (V-T1..V-T5, desucrage pentecostes dans mobile)
// ---------------------------------------------------------------------------

fn parse_transfers(slug: &str, from: u16, transfers: &[YamlTransfer])
    -> Result<Vec<TransferDef>, ForgeError>
{
    let mut result: Vec<TransferDef> = Vec::with_capacity(transfers.len());
    let mut seen: BTreeSet<&str> = BTreeSet::new();

    for t in transfers {
        // V-T3 — unicité de collides dans cette tranche temporelle
        for c in &t.collides {
            if !seen.insert(c.as_str()) {
            return Err(ParseError::TransferDuplicateCollides {
                slug:     slug.to_string(),
                from,
                collides: c.clone(),
            }.into());
            }
        }

        // V-T1 — exactement une option
        let count = t.offset.is_some() as u8
                  + t.date.is_some() as u8
                  + t.mobile.is_some() as u8;
        match count {
            0 => return Err(ParseError::TransferEmpty {
                slug: slug.to_string(), collides: t.collides.join(", ")
            }.into()),
            2.. => return Err(ParseError::TransferAmbiguous {
                slug: slug.to_string(), collides: t.collides.join(", ")
            }.into()),
            _ => {}
        }

        let target = if let Some(offset) = t.offset {
            // V-T4 — offset ≥ 1 (u32, seule valeur invalide = 0)
            if offset == 0 {
                return Err(ParseError::TransferOffsetNotPositive {
                    slug: slug.to_string(), collides: t.collides.join(", "), offset,
                }.into());
            }
            TransferTarget::Offset(offset)

        } else if let Some(ref d) = t.date {
            validate_date(slug, d.month, d.day)?;
            TransferTarget::Date { month: d.month, day: d.day }

        } else if let Some(ref m) = t.mobile {
            // V-T5 — ancre primitive uniquement
            if !PRIMITIVE_ANCHORS.contains(&m.anchor.as_str()) {
                return Err(ParseError::TransferMobileInvalidAnchor {
                    slug:    slug.to_string(),
                    collides: t.collides.join(", "),
                    anchor:  m.anchor.clone(),
                }.into());
            }
            // Desugaring pentecostes → pascha + 49
            let (anchor, offset) = if m.anchor == "pentecostes" {
                ("pascha".to_string(), m.offset + 49)
            } else {
                (m.anchor.clone(), m.offset)
            };
            TransferTarget::Mobile { anchor, offset }

        } else {
            unreachable!()
        };

        result.push(TransferDef { collides: t.collides.clone(), target });
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// parse_feast_from_yaml — noyau testable (expose pour tests)
// ---------------------------------------------------------------------------

pub fn parse_feast_from_yaml(
    slug:    &str,
    scope:   Scope,
    content: &str,
) -> Result<FeastDef, ForgeError> {
    // V1 — parsing YAML
    let yaml: YamlFeast = serde_yml::from_str(content)
        .map_err(|e| ParseError::MalformedYaml(e.to_string()))?;

    if yaml.version != 1 {
        return Err(ParseError::UnsupportedSchemaVersion(yaml.version).into());
    }

    // Temporalité — exactement un bloc ou aucun (delta pur)
    let temporality = match (yaml.date.as_ref(), yaml.mobile.as_ref()) {
        (Some(_), Some(_)) =>
            return Err(ParseError::AmbiguousTemporalityField { slug: slug.to_string() }.into()),
        (None, None) =>
            None,  // delta pur — temporalité héritée de l'universale au merge
        (Some(d), None) => {
            validate_date(slug, d.month, d.day)?;
            Some(Temporality::Fixed { month: d.month, day: d.day })
        }
        (None, Some(m)) => Some(parse_mobile_temporality(slug, m)?),
    };

    // Classe liturgique — ADR-038
    let class = yaml.class.as_deref().map(parse_class).transpose()?;

    let history = parse_history(slug, &yaml.history)?;

    Ok(FeastDef {
        slug: slug.to_string(),
        scope,
        category:    yaml.category,
        id:          yaml.id,
        temporality,
        class,
        history,
    })
}

// ---------------------------------------------------------------------------
// ingest_scope_dir — un scope (temporale/ + sanctorale/)
// INV-FORGE-1 : collecter → trier lex → ingérer
// ---------------------------------------------------------------------------

fn ingest_scope_dir(
    scope_dir: &Path,
    scope:     Scope,
    registry:  &mut FeastRegistry,
    is_base:   bool,   // true = universale, false = delta (merge)
) -> Result<(), ForgeError> {
    for sub in &["temporale", "sanctorale"] {
        let dir = scope_dir.join(sub);
        if !dir.exists() { continue; }

        // Collecte récursive — absorbe les sous-répertoires mensuels
        // (sanctorale/01/, sanctorale/03/…) sans changer la logique de tri.
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        collect_yaml_recursive(&dir, &mut files)?;
        files.sort(); // INV-FORGE-1 : ordre lexicographique global

        for path in files {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            validate_slug(&stem).map_err(ForgeError::Parse)?;

            let content = fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                continue;
            }

            let def = parse_feast_from_yaml(&stem, scope.clone(), &content)?;
            if is_base {
                registry.insert(def);
            } else {
                registry.merge(def);
            }
        }
    }
    Ok(())
}

fn collect_yaml_recursive(
    dir: &Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), ForgeError> {
    for entry in fs::read_dir(dir).map_err(ForgeError::Io)? {
        let entry = entry.map_err(ForgeError::Io)?;
        let path  = entry.path();
        if path.is_dir() {
            collect_yaml_recursive(&path, out)?;
        } else if matches!(
            path.extension().and_then(|x| x.to_str()),
            Some("yaml") | Some("yml")
        ) {
            out.push(path);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ingest_corpus — point d'entrée public
// ---------------------------------------------------------------------------

pub fn ingest_corpus(data_dir: &Path) -> Result<FeastRegistry, ForgeError> {
    let mut registry = FeastRegistry::new();

    // Universale
    let universale = data_dir.join("universale");
    if universale.exists() {
        ingest_scope_dir(&universale, Scope::Universal, &mut registry, true)?;
    }

    // Nationalia
    let nationalia = data_dir.join("nationalia");
    if nationalia.exists() {
        let iso_dirs = sorted_subdirs(&nationalia)?;
        for iso_path in iso_dirs {
            let iso = dir_name(&iso_path);
            ingest_scope_dir(&iso_path, Scope::National(iso), &mut registry, false)?;
        }
    }

    // Continentalia — traité comme National avec code continent
    let continentalia = data_dir.join("continentalia");
    if continentalia.exists() {
        let cont_dirs = sorted_subdirs(&continentalia)?;
        for cont_path in cont_dirs {
            let cont = dir_name(&cont_path);
            ingest_scope_dir(&cont_path, Scope::National(cont), &mut registry, false)?;
        }
    }

    // Dioecesana
    let dioecesana = data_dir.join("dioecesana");
    if dioecesana.exists() {
        let id_dirs = sorted_subdirs(&dioecesana)?;
        for id_path in id_dirs {
            let id = dir_name(&id_path);
            ingest_scope_dir(&id_path, Scope::Diocesan(id), &mut registry, false)?;
        }
    }

    // Ordines — traité comme Diocesan
    let ordines = data_dir.join("ordines");
    if ordines.exists() {
        let ordo_dirs = sorted_subdirs(&ordines)?;
        for ordo_path in ordo_dirs {
            // Ignorer i18n/ qui n'est pas un scope liturgique
            if dir_name(&ordo_path) == "i18n" { continue; }
            let ordo = dir_name(&ordo_path);
            ingest_scope_dir(&ordo_path, Scope::Diocesan(ordo), &mut registry, false)?;
        }
    }

    validate_collides_targets(&registry)?;

    Ok(registry)
}

// Helpers extraits pour éviter la répétition
fn sorted_subdirs(dir: &Path) -> Result<Vec<std::path::PathBuf>, ForgeError> {
    let mut dirs: Vec<_> = fs::read_dir(dir)
        .map_err(ForgeError::Io)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    Ok(dirs)
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

// ---------------------------------------------------------------------------
// V-T2 — vérification post-ingestion
// ---------------------------------------------------------------------------

fn validate_collides_targets(registry: &FeastRegistry) -> Result<(), ForgeError> {
    for feast in registry.iter() {
        for entry in &feast.history {
            for transfer in &entry.transfers {
                for c in &transfer.collides {
                    if !registry.contains(c.as_str()) {
                        return Err(ParseError::UnknownCollidesTarget {
                            slug:     feast.slug.clone(),
                            collides: c.clone(),
                        }.into());
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pipeline ID Allocation — Layout [SS|CC|IIIIIIIIIIII]
// ---------------------------------------------------------------------------

/// Mappe le Scope vers son invariant binaire (2 bits).
fn scope_to_bits(scope: &Scope) -> u8 {
    match scope {
        Scope::Universal => 0,
        Scope::National(_) => 1,
        Scope::Diocesan(_) => 2,
    }
}

pub fn allocate_feast_ids(
    registry: &FeastRegistry,
    lock: &mut FeastRegistryLock,
    lock_path: &Path,
) -> Result<BTreeMap<String, u16>, ForgeError> {
    let mut id_map: BTreeMap<String, u16> = BTreeMap::new();

    // Compteurs par (scope_bits, category) — next free sequence par bucket
    let mut counters: BTreeMap<(u8, u8), u16> = BTreeMap::new();

    // Slugs présents dans le YAML courant
    let current_slugs: BTreeSet<&str> = registry.iter().map(|f| f.slug.as_str()).collect();

    // Tombstone les slugs disparus du corpus
    for slug in lock.entries.keys().cloned().collect::<Vec<_>>() {
        if !current_slugs.contains(slug.as_str()) {
            lock.tombstone(&slug);
        }
    }

    // Itération lexicographique garantie (BTreeMap)
    for feast in registry.iter() {
        let slug = &feast.slug;
        let scope_bits = scope_to_bits(&feast.scope);   // 0=Universal, 1=National, 2=Diocesan
        let category   = feast.category;

        if let Some(lock_id) = lock.get(slug) {
            // Slug connu dans le lock — vérification conflit YAML.id
            if let Some(yaml_id) = feast.id {
                if yaml_id != lock_id {
                    return Err(ForgeError::FeastIDLockConflict {
                        slug:    slug.clone(),
                        yaml_id,
                        lock_id,
                    });
                }
            }
            id_map.insert(slug.clone(), lock_id);
        } else {
            // Nouveau slug — allouer le prochain ID libre dans (scope, category)
            let next = allocate_next(&mut counters, lock, scope_bits, category)?;
            lock.pin(slug, next);
            id_map.insert(slug.clone(), next);
        }
    }

    lock.save(lock_path)?;
    Ok(id_map)
}

fn allocate_next(
    counters: &mut BTreeMap<(u8, u8), u16>,
    lock: &FeastRegistryLock,
    scope: u8,
    category: u8,
) -> Result<u16, ForgeError> {
    let counter = counters.entry((scope, category)).or_insert(1u16);
    loop {
        let candidate = build_feast_id(scope, category, *counter);
        *counter += 1;
        if *counter > 4095 {
            return Err(ForgeError::FeastIDExhausted { scope, category });
        }
        // Sauter les IDs tombstonés
        if !lock.is_tombstoned_id(candidate) {
            return Ok(candidate);
        }
    }
}

fn build_feast_id(scope: u8, category: u8, sequence: u16) -> u16 {
    ((scope as u16) << 14) | ((category as u16) << 12) | (sequence & 0x0FFF)
}

// ---------------------------------------------------------------------------
// Tests unitaires
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- V6 ---

    #[test]
    fn slug_valid() {
        assert!(validate_slug("iosephi").is_ok());
        assert!(validate_slug("a1_b").is_ok());
    }

    #[test]
    fn slug_invalid_starts_digit() {
        assert!(matches!(
            validate_slug("1abc"),
            Err(ParseError::InvalidSlugSyntax(_))
        ));
    }

    #[test]
    fn slug_invalid_uppercase() {
        assert!(matches!(
            validate_slug("Abc"),
            Err(ParseError::InvalidSlugSyntax(_))
        ));
    }

    // --- Boundary Normalization : Precedence ---

    /// Une valeur YAML 0 (hors plage 1–13) doit être rejetée par Serde.
    #[test]
    fn precedence_yaml_zero_rejected() {
        let yaml = r#"
version: 1
category: 0
class: lord
mobile:
  anchor: pascha
  offset: 0
history:
  - precedence: 0
    nature: sollemnitas
    color: albus
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        // MalformedYaml car le message vient de serde::de::Error::custom
        assert!(matches!(err, ForgeError::Parse(ParseError::MalformedYaml(_))));
    }

    /// Une valeur YAML 14 (hors plage 1–13) doit être rejetée par Serde.
    #[test]
    fn precedence_yaml_fourteen_rejected() {
        let yaml = r#"
version: 1
category: 0
class: lord
mobile:
  anchor: pascha
  offset: 0
history:
  - precedence: 14
    nature: sollemnitas
    color: albus
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(err, ForgeError::Parse(ParseError::MalformedYaml(_))));
    }

    /// La valeur limite haute YAML 13 (→ 12 interne) doit être acceptée.
    #[test]
    fn precedence_yaml_thirteen_accepted() {
        let yaml = r#"
version: 1
category: 1
class: saint
date:
  month: 5
  day: 1
history:
  - precedence: 13
    nature: feria
    color: viridis
"#;
        let def = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap();
        assert_eq!(def.history[0].precedence, Some(12)); // 13 − 1 = 12 interne
    }

    /// La valeur limite basse YAML 1 (→ 0 interne) doit être acceptée.
    #[test]
    fn precedence_yaml_one_maps_to_zero_internal() {
        let yaml = r#"
version: 1
category: 0
class: lord
mobile:
  anchor: pascha
  offset: 0
history:
  - precedence: 1
    nature: sollemnitas
    color: albus
"#;
        let def = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap();
        assert_eq!(def.history[0].precedence, Some(0)); // 1 − 1 = 0 interne = TriduumSacrum
    }

    #[test]
    fn precedence_yaml_thirteen_maps_to_twelve_internal() {
        let yaml = r#"
version: 1
category: 1
class: saint
date:
  month: 5
  day: 1
history:
  - precedence: 13
    nature: feria
    color: viridis
"#;
        let def = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap();
        assert_eq!(def.history[0].precedence, Some(12));
    }

    #[test]
    fn v4a_offset_on_ordinal_anchor() {
        let yaml = r#"
version: 1
category: 0
class: lord
mobile:
  anchor: tempus_ordinarium
  offset: 7
  ordinal: 3
history:
  - precedence: 2
    nature: sollemnitas
    color: albus
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(err, ForgeError::Parse(ParseError::OffsetOnOrdinalAnchor { .. })));
    }

    #[test]
    fn v4a_ordinal_on_non_ordinal_anchor() {
        let yaml = r#"
version: 1
category: 0
class: lord
mobile:
  anchor: pascha
  ordinal: 1
history:
  - precedence: 2
    nature: sollemnitas
    color: albus
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(err, ForgeError::Parse(ParseError::OrdinalOnNonOrdinalAnchor { .. })));
    }

    // --- V-Natura-Memoria ---

    /// YAML precedence: 10 → interne 9 (FestaPropria) : invalide pour memoria.
    #[test]
    fn v_natura_memoria_invalid_precedence() {
        let yaml = r#"
version: 1
category: 1
class: lord
date:
  month: 5
  day: 1
history:
  - precedence: 8
    nature: memoria
    color: albus
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(
            err,
            ForgeError::Parse(ParseError::InvalidMemoriaPrecedence {
                found_precedence: 7, // valeur interne après shift
                ..
            })
        ));
    }

    /// YAML 10 -> interne 9 (MemoriaeObligatoriaGenerales) : valide.
    #[test]
    fn v_natura_memoria_valid_obligatoria_generales() {
        let yaml = r#"
version: 1
category: 1
class: lord
date:
  month: 5
  day: 1
history:
  - precedence: 10
    nature: memoria
    color: albus
"#;
        assert!(parse_feast_from_yaml("test_slug", Scope::Universal, yaml).is_ok());
    }
 
    /// YAML 11 -> interne 10 (MemoriaeObligatoriaePropria) : valide.
    #[test]
    fn v_natura_memoria_valid_obligatoria_propria() {
        let yaml = r#"
version: 1
category: 1
class: lord
date:
  month: 5
  day: 1
history:
  - precedence: 11
    nature: memoria
    color: albus
"#;
        assert!(parse_feast_from_yaml("test_slug", Scope::Universal, yaml).is_ok());
    }
 
    /// YAML 12 -> interne 11 (MemoriaeAdLibitum) : valide.
    #[test]
    fn v_natura_memoria_valid_ad_libitum() {
        let yaml = r#"
version: 1
category: 1
class: lord
date:
  month: 5
  day: 1
history:
  - precedence: 12
    nature: memoria
    color: albus
"#;
        assert!(parse_feast_from_yaml("test_slug", Scope::Universal, yaml).is_ok());
    }

    // --- V-Vigilia ---

    /// YAML precedence: 12 → interne 11. Memoria + has_vigil_mass → VigiliaNonSollemnitas.
    #[test]
    fn v_vigilia_non_sollemnitas() {
        let yaml = r#"
version: 1
category: 1
class: lord
date:
  month: 5
  day: 1
history:
  - precedence: 12
    nature: memoria
    color: albus
    has_vigil_mass: true
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(err, ForgeError::Parse(ParseError::VigiliaNonSollemnitas { .. })));
    }

    // --- Desugaring pentecostes (temporalité) ---

    #[test]
    fn desugaring_pentecostes_temporality() {
        let yaml = r#"
version: 1
category: 0
class: lord
mobile:
  anchor: pentecostes
  offset: 0
history:
  - precedence: 2
    nature: sollemnitas
    color: albus
"#;
        let def = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap();
        match def.temporality.expect("temporality attendue dans ce test") {
            Temporality::Mobile { anchor, offset } => {
                assert_eq!(anchor, "pascha");
                assert_eq!(offset, 49);
            }
            _ => panic!("expected Mobile"),
        }
    }

    // --- V-T1 ---

    #[test]
    fn transfer_ambiguous() {
        let yaml = r#"
version: 1
category: 1
class: lord
date:
  month: 3
  day: 19
history:
  - precedence: 2
    nature: sollemnitas
    color: albus
    transfers:
      - collides: other_slug
        offset: 2
        mobile:
          anchor: pascha
          offset: 3
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(err, ForgeError::Parse(ParseError::TransferAmbiguous { .. })));
    }

    // --- V-T5 ---

    #[test]
    fn transfer_mobile_invalid_anchor_tempus_ordinarium() {
        let yaml = r#"
version: 1
category: 1
class: lord
date:
  month: 3
  day: 19
history:
  - precedence: 2
    nature: sollemnitas
    color: albus
    transfers:
      - collides: other_slug
        mobile:
          anchor: tempus_ordinarium
          offset: 0
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(
            err,
            ForgeError::Parse(ParseError::TransferMobileInvalidAnchor { .. })
        ));
    }

    // --- V-T4 ---

    #[test]
    fn transfer_offset_zero_rejected() {
        let yaml = r#"
version: 1
category: 1
class: lord
date:
  month: 3
  day: 19
history:
  - precedence: 2
    nature: sollemnitas
    color: albus
    transfers:
      - collides: other_slug
        offset: 0
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(
            err,
            ForgeError::Parse(ParseError::TransferOffsetNotPositive { offset: 0, .. })
        ));
    }

    // --- Desugaring pentecostes dans transfer.mobile ---

    #[test]
    fn desugaring_pentecostes_in_transfer_mobile() {
        let yaml = r#"
version: 1
category: 1
class: lord
date:
  month: 3
  day: 19
history:
  - precedence: 2
    nature: sollemnitas
    color: albus
    transfers:
      - collides: other_slug
        mobile:
          anchor: pentecostes
          offset: 3
"#;
        let def = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap();
        let t = &def.history[0].transfers[0];
        match &t.target {
            TransferTarget::Mobile { anchor, offset } => {
                assert_eq!(anchor, "pascha");
                assert_eq!(*offset, 52); // 49 + 3
            }
            _ => panic!("expected Mobile"),
        }
    }

    // --- UnsupportedSchemaVersion ---

    #[test]
    fn unsupported_schema_version() {
        let yaml = r#"
version: 2
category: 1
class: lord
date:
  month: 1
  day: 1
history:
  - precedence: 2
    nature: sollemnitas
    color: albus
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(err, ForgeError::Parse(ParseError::UnsupportedSchemaVersion(2))));
    }

    // --- parse_class — ADR-038 ---

    #[test]
    fn class_lord_parsed() {
        let yaml = r#"
version: 1
category: 0
class: lord
mobile:
  anchor: pascha
  offset: 68
history:
  - precedence: 3
    nature: sollemnitas
    color: albus
"#;
        let def = parse_feast_from_yaml("sacratissimi_cordis", Scope::Universal, yaml).unwrap();
        assert_eq!(def.class, Some(LiturgicalClass::Lord));
    }

    #[test]
    fn class_saint_parsed() {
        let yaml = r#"
version: 1
category: 1
class: saint
date:
  month: 6
  day: 29
history:
  - precedence: 3
    nature: sollemnitas
    color: rubeus
"#;
        let def = parse_feast_from_yaml("petri_et_pauli", Scope::Universal, yaml).unwrap();
        assert_eq!(def.class, Some(LiturgicalClass::Saint));
    }

    #[test]
    fn class_absent_yields_none() {
        let yaml = r#"
version: 1
category: 1
date:
  month: 5
  day: 1
history:
  - precedence: 10
    nature: memoria
    color: albus
"#;
        let def = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap();
        assert_eq!(def.class, None);
    }

    #[test]
    fn class_unknown_rejected() {
        let yaml = r#"
version: 1
category: 1
class: angel
date:
  month: 5
  day: 1
history:
  - precedence: 3
    nature: sollemnitas
    color: albus
"#;
        let err = parse_feast_from_yaml("test_slug", Scope::Universal, yaml).unwrap_err();
        assert!(matches!(err, ForgeError::Registry(RegistryError::UnknownClassString(_))));
    }

    // --- Iosephi — transfers scoped (schème v1.7.0) ---
    //
    // YAML precedence: 5 → interne 4 (SollemnitatesGenerales).

    const YAML_IOSEPHI: &str = r#"
version: 1
category: 1
class: saint
date:
  month: 3
  day: 19
history:
  - from: 1969
    to: 2007
    precedence: 3
    nature: sollemnitas
    color: albus
    # Comportement standard par défaut : Incrémentation déterministe (J+1) gérée par le moteur
    transfers:
      # Semaine Sainte : déplacement post-Octave
      - collides:
          - dominica_in_palmis_de_passione_domini # Rameaux
          - feria_ii_hebdomadae_sanctae # Lundi Saint
          - feria_iii_hebdomadae_sanctae # Mardi Saint
          - feria_iv_hebdomadae_sanctae # Mercredi Saint
          - feria_v_hebdomadae_sanctae # Jeudi Saint
          - feria_vi_hebdomadae_sanctae # Vendredi Saint
          - sabbato_sancto # Samedi Saint
          - dominica_resurrectionis # Dimanche de Pâques
        mobile:
          anchor: pascha
          offset: 8
  - from: 2008
    precedence: 3
    nature: sollemnitas
    color: albus
    transfers:
      # Semaine Sainte : déplacement rétrograde
      # Cible unique : Samedi avant les Rameaux (Easter - 8)
      - collides:
          - dominica_in_palmis_de_passione_domini # Rameaux
          - feria_ii_hebdomadae_sanctae # Lundi Saint
          - feria_iii_hebdomadae_sanctae # Mardi Saint
          - feria_iv_hebdomadae_sanctae # Mercredi Saint
          - feria_v_hebdomadae_sanctae # Jeudi Saint
          - feria_vi_hebdomadae_sanctae # Vendredi Saint
          - sabbato_sancto # Samedi Saint
          - dominica_resurrectionis # Dimanche de Pâques
        mobile:
          anchor: pascha
          offset: -8
"#;

    #[test]
    fn parse_iosephi_scoped_transfers() {
        let feast = parse_feast_from_yaml(
            "iosephi_sponsi_beatae_mariae_virginis",
            Scope::Universal,
            YAML_IOSEPHI,
        ).expect("parse doit réussir");

        assert_eq!(feast.history.len(), 2);

        let v1969 = &feast.history[0];
        assert_eq!(v1969.transfers.len(), 1, "1 TransferDef multi-collides");
        assert_eq!(v1969.transfers[0].collides.len(), 8, "8 slugs Semaine Sainte (post-Octave)");

        let v2008 = &feast.history[1];
        assert_eq!(v2008.transfers.len(), 1, "1 TransferDef multi-collides");
        assert_eq!(v2008.transfers[0].collides.len(), 8, "8 slugs Semaine Sainte (pré-Octave)");

        for t in &v2008.transfers {
            match &t.target {
                TransferTarget::Mobile { anchor, offset } => {
                    assert_eq!(anchor, "pascha");
                    assert_eq!(*offset, -8i32);
                }
                _ => panic!("attendu TransferTarget::Mobile"),
            }
        }
    }
}
