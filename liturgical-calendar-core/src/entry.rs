use core::mem::{offset_of, size_of};

use crate::types::{Color, DomainError, LiturgicalPeriod, Nature, Precedence};

/// Version du format `.kald` — v5 : séparation Feast Registry / Timeline.
///
/// Synchronisée avec `header[4..6]` (packing.rs) et la garde de `validate_header`.
pub const KALD_FORMAT_VERSION: u16 = 5;

// ── FeastEntry ────────────────────────────────────────────────────────────────

/// Invariants d'une fête — 4 octets, stride constant, little-endian.
///
/// Layout `flags` (u16) :
/// - bits [3:0]   → `Precedence`       (0–12)
/// - bits [7:4]   → `Color`            (0–5)
/// - bits [10:8]  → `LiturgicalPeriod` (0–6)
/// - bits [13:11] → `Nature`           (0–4)
/// - bit  [14]    → `has_vigil_mass`   — invariant corpus (Messe de Vigile propre)
/// - bit  [15]    → réservé, doit être nul
///
/// Indexé par `registry_index` (1-based) depuis la Timeline ou le Secondary Pool.
/// `registry_index == 0` est le sentinel Padding — jamais une entrée valide.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct FeastEntry {
    /// Identifiant corpus — vérification croisée `.lits`.
    /// Non utilisé pour le calcul d'index (positionnement dans l'array = registry_index − 1).
    pub feast_id: u16,
    /// Invariants : Precedence, Color, LiturgicalPeriod, Nature, has_vigil_mass.
    pub flags: u16,
}

const _: () = assert!(size_of::<FeastEntry>() == 4);
const _: () = assert!(offset_of!(FeastEntry, flags) == 2);

impl FeastEntry {
    /// Retourne une entrée entièrement nulle.
    pub const fn zeroed() -> Self {
        Self { feast_id: 0, flags: 0 }
    }

    /// Extrait la `Precedence` depuis `flags[3:0]`.
    #[inline]
    pub fn precedence(&self) -> Result<Precedence, DomainError> {
        Precedence::try_from_u8((self.flags & 0x000F) as u8)
    }

    /// Extrait la `Color` depuis `flags[7:4]`.
    #[inline]
    pub fn color(&self) -> Result<Color, DomainError> {
        Color::try_from_u8(((self.flags >> 4) & 0x000F) as u8)
    }

    /// Extrait le `LiturgicalPeriod` depuis `flags[10:8]`.
    #[inline]
    pub fn liturgical_period(&self) -> Result<LiturgicalPeriod, DomainError> {
        LiturgicalPeriod::try_from_u8(((self.flags >> 8) & 0x0007) as u8)
    }

    /// Extrait la `Nature` depuis `flags[13:11]`.
    #[inline]
    pub fn nature(&self) -> Result<Nature, DomainError> {
        Nature::try_from_u8(((self.flags >> 11) & 0x0007) as u8)
    }

    /// `true` si la fête a une Messe de Vigile propre — invariant corpus.
    /// La Vigile effective pour un soir donné est lue via `TimelineEntry.has_vigilia()`.
    #[inline]
    pub fn has_vigil_mass(&self) -> bool {
        self.flags & (1 << 14) != 0
    }
}

impl Default for FeastEntry {
    fn default() -> Self {
        Self::zeroed()
    }
}

// ── TimelineEntry ─────────────────────────────────────────────────────────────

/// Occurrence journalière — stride constant 8 octets, little-endian.
///
/// `primary_index` : registry_index de la fête principale (1-based).
/// - `0` = Padding Entry (aucune célébration pour ce slot).
/// - `1..=registry_count` : index valide dans le Feast Registry.
///
/// `occurrence_flags` :
/// - bit 0 : `has_vesperae_i` — ce soir civil commence les Premières Vêpres de DOY+1.
/// - bit 1 : `has_vigilia`    — ce soir civil a une Messe de Vigile propre (DOY+1).
///   Ces bits sont exclusivement positionnés par `vespers_lookahead_pass`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct TimelineEntry {
    /// 0 = Padding Entry. Sinon : registry_index de la fête principale (1-based).
    pub primary_index:    u16,
    /// Offset (en nombre de u16) dans le Secondary Pool.
    pub secondary_offset: u16,
    /// Bits d'occurrence : bit 0 = vesperae_i, bit 1 = vigilia.
    pub occurrence_flags: u8,
    /// Nombre de célébrations secondaires à lire depuis le Secondary Pool.
    pub secondary_count:  u8,
    /// Padding structurel — doit être nul.
    pub _reserved:        u16,
}

// Assertions statiques de layout.
const _: () = assert!(size_of::<TimelineEntry>() == 8);
const _: () = assert!(offset_of!(TimelineEntry, secondary_offset) == 2);
const _: () = assert!(offset_of!(TimelineEntry, occurrence_flags) == 4);
const _: () = assert!(offset_of!(TimelineEntry, secondary_count)  == 5);
const _: () = assert!(offset_of!(TimelineEntry, _reserved)        == 6);

/// Discriminant de layout — capturant taille et offsets de `TimelineEntry` + version.
///
/// Calculé entièrement à compile-time via `const {}`.
/// Invalide tous les artefacts `.kald` v4 sur un Engine v5 sans intervention manuelle.
/// Inscrit dans `header[68..76]` par la Forge.
/// Vérifié par `validate_header` avant tout accès au Data Body.
pub const LAYOUT_DISCRIMINANT: u64 = {
    let sz             = size_of::<TimelineEntry>() as u64;                      // 8
    let off_sec_offset = offset_of!(TimelineEntry, secondary_offset) as u64;    // 2
    let off_occ_flags  = offset_of!(TimelineEntry, occurrence_flags) as u64;    // 4
    let off_sec_count  = offset_of!(TimelineEntry, secondary_count) as u64;     // 5
    let off_reserved   = offset_of!(TimelineEntry, _reserved) as u64;           // 6
    let version        = KALD_FORMAT_VERSION as u64;                             // 5

    sz
        ^ (off_sec_offset << 8)
        ^ (off_occ_flags  << 16)
        ^ (off_sec_count  << 24)
        ^ (off_reserved   << 32)
        ^ (version        << 48)
};

impl TimelineEntry {
    /// Retourne une entrée entièrement nulle (Padding Entry).
    ///
    /// `const fn` — utilisable en contexte `no_alloc`.
    pub const fn zeroed() -> Self {
        Self {
            primary_index:    0,
            secondary_offset: 0,
            occurrence_flags: 0,
            secondary_count:  0,
            _reserved:        0,
        }
    }

    /// `true` si `primary_index == 0` (aucune célébration pour ce slot).
    #[inline]
    pub fn is_padding(&self) -> bool {
        self.primary_index == 0
    }

    /// `true` si ce soir civil commence les Premières Vêpres de la fête de DOY+1.
    /// Consulter `kal_read_feast(primary_index de DOY+1)` pour les invariants.
    #[inline]
    pub fn has_vesperae_i(&self) -> bool {
        self.occurrence_flags & (1 << 0) != 0
    }

    /// `true` si ce soir civil a une Messe de Vigile propre pour la fête de DOY+1.
    #[inline]
    pub fn has_vigilia(&self) -> bool {
        self.occurrence_flags & (1 << 1) != 0
    }
}

impl Default for TimelineEntry {
    fn default() -> Self {
        Self::zeroed()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_feast_entry_size() {
        assert_eq!(size_of::<FeastEntry>(), 4);
    }

    #[test]
    fn layout_timeline_entry_size() {
        assert_eq!(size_of::<TimelineEntry>(), 8);
    }

    #[test]
    fn layout_timeline_occurrence_flags_offset() {
        assert_eq!(offset_of!(TimelineEntry, occurrence_flags), 4);
    }

    #[test]
    fn layout_timeline_reserved_offset() {
        assert_eq!(offset_of!(TimelineEntry, _reserved), 6);
    }

    #[test]
    fn timeline_zeroed_is_padding() {
        let e = TimelineEntry::zeroed();
        assert!(e.is_padding());
        assert!(!e.has_vesperae_i());
        assert!(!e.has_vigilia());
    }

    #[test]
    fn timeline_default_eq_zeroed() {
        assert_eq!(TimelineEntry::default(), TimelineEntry::zeroed());
    }

    #[test]
    fn feast_entry_flags_roundtrip() {
        use crate::types::{Color, LiturgicalPeriod, Nature, Precedence};

        let p  = Precedence::MemoriaeAdLibitum as u16;     // 11
        let c  = Color::Viridis as u16;                    // 2
        let lp = LiturgicalPeriod::TempusOrdinarium as u16; // 0
        let n  = Nature::Memoria as u16;                   // 2
        let vigil: u16 = 1 << 14;

        let flags = p | (c << 4) | (lp << 8) | (n << 11) | vigil;
        let fe = FeastEntry { feast_id: 42, flags };

        assert_eq!(fe.precedence(),       Ok(Precedence::MemoriaeAdLibitum));
        assert_eq!(fe.color(),            Ok(Color::Viridis));
        assert_eq!(fe.liturgical_period(),Ok(LiturgicalPeriod::TempusOrdinarium));
        assert_eq!(fe.nature(),           Ok(Nature::Memoria));
        assert!(fe.has_vigil_mass());
    }

    #[test]
    fn occurrence_flags_bits() {
        let mut e = TimelineEntry::zeroed();
        e.primary_index = 1;

        e.occurrence_flags = 0b01;
        assert!(e.has_vesperae_i());
        assert!(!e.has_vigilia());

        e.occurrence_flags = 0b10;
        assert!(!e.has_vesperae_i());
        assert!(e.has_vigilia());

        e.occurrence_flags = 0b11;
        assert!(e.has_vesperae_i());
        assert!(e.has_vigilia());
    }

    #[test]
    fn layout_discriminant_nonzero_and_encodes_version() {
        // Le discriminant doit être non-nul et changer si la version change.
        assert_ne!(LAYOUT_DISCRIMINANT, 0);
        // Bits [55:48] = version (5) XOR bits [7:0] = sz (8) : valeur détectable.
        let version_bits = (LAYOUT_DISCRIMINANT >> 48) & 0xFF;
        assert_eq!(version_bits, KALD_FORMAT_VERSION as u64);
    }
}
