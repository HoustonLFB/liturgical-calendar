use core::mem::{offset_of, size_of};

use crate::types::{Color, DomainError, LiturgicalPeriod, Nature, Precedence};

/// Entrée calendrier — stride constant 8 octets, little-endian.
///
/// Layout `flags` (u16) :
/// - bits [3:0]   → `Precedence`       (0–12)
/// - bits [7:4]   → `Color`            (0–5)
/// - bits [10:8]  → `LiturgicalPeriod` (0–6)
/// - bits [13:11] → `Nature`           (0–4)
/// - bits [15:14] → réservés, doivent être nuls
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct CalendarEntry {
    /// `0` = Padding Entry (aucune célébration).
    pub primary_id: u16,
    /// Index dans le Secondary Pool.
    pub secondary_index: u16,
    /// Champ de bits encodant Precedence, Color, LiturgicalPeriod, Nature.
    pub flags: u16,
    /// Nombre de célébrations secondaires.
    pub secondary_count: u8,
    /// Padding de structure — doit être nul.
    pub _reserved: u8,
}

// Assertions statiques de layout.
const _: () = assert!(size_of::<CalendarEntry>() == 8);
const _: () = assert!(offset_of!(CalendarEntry, flags) == 4);
const _: () = assert!(offset_of!(CalendarEntry, secondary_count) == 6);

/// Version du format `.kald` — source unique partagée entre la Forge et l'Engine.
/// Synchronisée avec `header[4..6]` (packing.rs) et la garde de `validate_header`.
pub const KALD_FORMAT_VERSION: u16 = 4;

/// Discriminant de layout — capturant taille et offsets de `CalendarEntry` + version.
///
/// Calculé entièrement à compile-time via `const {}`.
/// Toute modification de padding, réordonnancement de champs ou bump de version
/// invalide ce discriminant sans intervention manuelle.
///
/// Inscrit dans `header[56..64]` par la Forge.
/// Vérifié par `validate_header` avant tout accès au Data Body.
pub const LAYOUT_DISCRIMINANT: u64 = {
    let sz            = size_of::<CalendarEntry>() as u64;
    let off_secondary = offset_of!(CalendarEntry, secondary_index) as u64;
    let off_flags     = offset_of!(CalendarEntry, flags) as u64;
    let off_sec_count = offset_of!(CalendarEntry, secondary_count) as u64;
    let off_reserved  = offset_of!(CalendarEntry, _reserved) as u64;
    let version       = KALD_FORMAT_VERSION as u64;

    sz
        ^ (off_secondary << 8)
        ^ (off_flags     << 16)
        ^ (off_sec_count << 24)
        ^ (off_reserved  << 32)
        ^ (version       << 48)
};

impl CalendarEntry {
    /// Retourne une entrée entièrement nulle (Padding Entry).
    ///
    /// `const fn` — utilisable en contexte `no_alloc`.
    pub const fn zeroed() -> Self {
        Self {
            primary_id: 0,
            secondary_index: 0,
            flags: 0,
            secondary_count: 0,
            _reserved: 0,
        }
    }

    /// `true` si `primary_id == 0` (aucune célébration pour ce jour).
    #[inline]
    pub fn is_padding(&self) -> bool {
        self.primary_id == 0
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

    /// `true` si ce soir civil commence les Premières Vêpres de la fête de DOY+1.
    /// Consulter `kal_read_entry(year, doy+1)` pour les détails de cette fête.
    pub fn has_vesperae_i(&self) -> bool {
        self.flags & (1 << 14) != 0
    }

    /// `true` si ce soir civil a une Messe de Vigile propre pour la fête de DOY+1.
    pub fn has_vigilia(&self) -> bool {
        self.flags & (1 << 15) != 0
    }
}

impl Default for CalendarEntry {
    fn default() -> Self {
        Self::zeroed()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_entry_size() {
        assert_eq!(size_of::<CalendarEntry>(), 8);
    }

    #[test]
    fn layout_flags_offset() {
        assert_eq!(offset_of!(CalendarEntry, flags), 4);
    }

    #[test]
    fn layout_secondary_count_offset() {
        assert_eq!(offset_of!(CalendarEntry, secondary_count), 6);
    }

    #[test]
    fn zeroed_is_padding() {
        let e = CalendarEntry::zeroed();
        assert!(e.is_padding());
        assert_eq!(e.flags, 0);
    }

    #[test]
    fn default_eq_zeroed() {
        assert_eq!(CalendarEntry::default(), CalendarEntry::zeroed());
    }

    #[test]
    fn flags_encoding_roundtrip() {
        use crate::types::{Color, LiturgicalPeriod, Nature, Precedence};

        let p = Precedence::MemoriaeObligatoriae as u16; // 11
        let c = Color::Viridis as u16; // 2
        let lp = LiturgicalPeriod::TempusOrdinarium as u16; // 0
        let n = Nature::Memoria as u16; // 2

        let flags = p | (c << 4) | (lp << 8) | (n << 11);

        let entry = CalendarEntry {
            primary_id: 1,
            secondary_index: 0,
            flags,
            secondary_count: 0,
            _reserved: 0,
        };

        assert_eq!(entry.precedence(), Ok(Precedence::MemoriaeObligatoriae));
        assert_eq!(entry.color(), Ok(Color::Viridis));
        assert_eq!(
            entry.liturgical_period(),
            Ok(LiturgicalPeriod::TempusOrdinarium)
        );
        assert_eq!(entry.nature(), Ok(Nature::Memoria));
    }
}
