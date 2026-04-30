use std::path::PathBuf;
use clap::Parser;

use liturgical_calendar_core::{
    kal_read_entry, kal_read_secondary, KAL_ENGINE_OK,
    lits_provider::{LitsProvider, LitsError},
    entry::CalendarEntry,
    types::{Precedence, Color, LiturgicalPeriod, Nature},
};

// ── Arguments CLI ─────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name  = "kal-read",
    about = "Lit une entrée d'un fichier .kald (+ label .lits optionnel)",
)]
struct Args {
    /// Chemin vers le fichier `.kald`.
    #[arg(long)]
    kald: PathBuf,

    /// Année calendaire (1969–2399).
    #[arg(long)]
    year: u16,

    /// Jour de l'année (0 = 1er janvier, 365 = 31 décembre ou Padding Entry).
    #[arg(long)]
    doy: u16,

    /// Chemin vers le fichier `.lits` (optionnel — affiche label + annotation).
    #[arg(long)]
    lits: Option<PathBuf>,
}

// ── Point d'entrée ────────────────────────────────────────────────────────────

fn main() {
    let args = Args::parse();

    if let Err(e) = run(&args) {
        eprintln!("[kal-read] erreur : {e}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), String> {
    // ── Chargement du .kald ───────────────────────────────────────────────────
    let kald = std::fs::read(&args.kald)
        .map_err(|e| format!("lecture {} : {e}", args.kald.display()))?;

    // ── Lecture de l'entrée primaire ──────────────────────────────────────────
    let mut entry = CalendarEntry::zeroed();
    let rc = unsafe {
        kal_read_entry(
            kald.as_ptr(), kald.len(),
            args.year, args.doy,
            &mut entry,
        )
    };

    if rc != KAL_ENGINE_OK {
        return Err(format!("kal_read_entry : code={rc}"));
    }

    // ── Affichage entrée primaire ─────────────────────────────────────────────
    println!("year={year}  doy={doy}", year = args.year, doy = args.doy);

    if entry.is_padding() {
        println!("  [Padding Entry — aucune célébration]");
        return Ok(());
    }

    println!("  feast_id    : {:#06x}", entry.primary_id);
    println!("  flags       : {:#06x}", entry.flags);
    println!("  precedence  : {}", fmt_precedence(entry.precedence()));
    println!("  color       : {}", fmt_color(entry.color()));
    println!("  period      : {}", fmt_period(entry.liturgical_period()));
    println!("  nature      : {}", fmt_nature(entry.nature()));
    println!("  secondary   : {} entrée(s) (index={})",
        entry.secondary_count, entry.secondary_index);
    println!("  vesperae_i  : {}", entry.has_vesperae_i());
    println!("  vigilia     : {}", entry.has_vigilia());

    // ── Entrées secondaires ───────────────────────────────────────────────────
    let mut sec_ids: Vec<u16> = Vec::new();

    if entry.secondary_count > 0 {
        sec_ids.resize(entry.secondary_count as usize, 0u16);
        let rc = unsafe {
            kal_read_secondary(
                kald.as_ptr(), kald.len(),
                entry.secondary_index,
                entry.secondary_count,
                sec_ids.as_mut_ptr(),
                entry.secondary_count,
            )
        };
        if rc != KAL_ENGINE_OK {
            return Err(format!("kal_read_secondary : code={rc}"));
        }

        println!();
        println!("  Célébrations secondaires :");
        for (i, &sid) in sec_ids.iter().enumerate() {
            println!("    [{}] feast_id={:#06x}", i, sid);
        }
    }

    // ── Label .lits (optionnel) ───────────────────────────────────────────────
    if let Some(lits_path) = &args.lits {
        let lits = std::fs::read(lits_path)
            .map_err(|e| format!("lecture {} : {e}", lits_path.display()))?;

        // Vérification cohérence build_id kald / lits.
        let kald_build_id = &kald[24..32]; // checksum[..8]
        let provider = LitsProvider::new(&lits).map_err(fmt_lits_error)?;

        if provider.build_id() != kald_build_id {
            return Err(format!(
                "build_id mismatch — .kald={} .lits={}",
                hex8(kald_build_id),
                hex8(provider.build_id()),
            ));
        }

        println!();
        match provider.get(entry.primary_id, args.year) {
            Some(lits_entry) => {
                println!("  label       : {}", lits_entry.label);
                match lits_entry.annotation {
                    Some(ann) => println!("  annotation  : {}", ann),
                    None      => println!("  annotation  : —"),
                }
            }
            None => println!("  [label absent pour feast_id={:#06x} year={}]",
                entry.primary_id, args.year),
        }

        // Labels secondaires
        if !sec_ids.is_empty() {
            println!();
            println!("  Labels secondaires :");
            for (i, &sid) in sec_ids.iter().enumerate() {
                match provider.get(sid, args.year) {
                    Some(e) => println!("    [{}] {}", i, e.label),
                    None    => println!("    [{}] feast_id={:#06x} — label absent", i, sid),
                }
            }
        }
    }

    Ok(())
}

// ── Helpers d'affichage ───────────────────────────────────────────────────────

fn fmt_precedence(r: Result<Precedence, impl std::fmt::Debug>) -> String {
    match r {
        Ok(p)  => format!("{:?} ({})", p, p as u8),
        Err(e) => format!("invalide ({:?})", e),
    }
}

fn fmt_color(r: Result<Color, impl std::fmt::Debug>) -> String {
    match r {
        Ok(c)  => format!("{:?} ({})", c, c as u8),
        Err(e) => format!("invalide ({:?})", e),
    }
}

fn fmt_period(r: Result<LiturgicalPeriod, impl std::fmt::Debug>) -> String {
    match r {
        Ok(p)  => format!("{:?} ({})", p, p as u8),
        Err(e) => format!("invalide ({:?})", e),
    }
}

fn fmt_nature(r: Result<Nature, impl std::fmt::Debug>) -> String {
    match r {
        Ok(n)  => format!("{:?} ({})", n, n as u8),
        Err(e) => format!("invalide ({:?})", e),
    }
}

fn fmt_lits_error(e: LitsError) -> String {
    match e {
        LitsError::BufferTooShort        => "LitsError: buffer trop court".to_owned(),
        LitsError::InvalidMagic          => "LitsError: magic invalide".to_owned(),
        LitsError::UnsupportedVersion(v) => format!("LitsError: version {v} non supportée"),
        LitsError::CorruptLayout         => "LitsError: layout corrompu".to_owned(),
    }
}

fn hex8(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
