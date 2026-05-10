use std::path::PathBuf;
use clap::Parser;

use liturgical_calendar_core::{
    kal_read_entry, kal_read_feast, kal_read_secondary, KAL_ENGINE_OK,
    lits_provider::{LitsProvider, LitsEntry, LitsError},
    entry::{FeastEntry, TimelineEntry},
    types::{Precedence, Color, LiturgicalPeriod, Nature},
};

// ── Arguments CLI ─────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "kal-read", about = "Lit une entrée d'un fichier .kald v5")]
struct Args {
    #[arg(long)] kald: PathBuf,
    #[arg(long)] year: u16,
    #[arg(long)] doy:  u16,
    #[arg(long)] lits: Option<PathBuf>,
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
    let kald = std::fs::read(&args.kald)
        .map_err(|e| format!("lecture {} : {e}", args.kald.display()))?;

    // Charger les octets .lits avant tout — le LitsProvider emprunte cette mémoire.
    let lits_bytes: Option<Vec<u8>> = args.lits.as_ref()
        .map(|p| std::fs::read(p).map_err(|e| format!("lecture {} : {e}", p.display())))
        .transpose()?;

    let ptr = kald.as_ptr();
    let len = kald.len();

    // ── TimelineEntry ─────────────────────────────────────────────────────────
    let mut entry = TimelineEntry::zeroed();
    let rc = unsafe { kal_read_entry(ptr, len, args.year, args.doy, &mut entry) };
    if rc != KAL_ENGINE_OK {
        return Err(format!("kal_read_entry : code={rc}"));
    }

    println!("year={year}  doy={doy}", year = args.year, doy = args.doy);

    if entry.is_padding() {
        println!("  [Padding Entry — aucune célébration]");
        return Ok(());
    }

    // ── FeastEntry primaire ───────────────────────────────────────────────────
    let mut primary_feast = FeastEntry::zeroed();
    let rc = unsafe { kal_read_feast(ptr, len, entry.primary_index, &mut primary_feast) };
    if rc != KAL_ENGINE_OK {
        return Err(format!("kal_read_feast (index={}) : code={rc}", entry.primary_index));
    }

    // ── Provider .lits ────────────────────────────────────────────────────────
    // Créé après validation — emprunte lits_bytes qui vit pour toute la fonction.
    let provider: Option<LitsProvider<'_>> = lits_bytes.as_ref()
        .map(|bytes| {
            // En v5 : checksum = kald[36..68], build_id = kald[36..44].
            let kald_build_id = &kald[36..44];
            let p = LitsProvider::new(bytes).map_err(fmt_lits_error)?;
            if p.build_id() != kald_build_id {
                return Err(format!(
                    "build_id mismatch — .kald={} .lits={}",
                    hex8(kald_build_id), hex8(p.build_id()),
                ));
            }
            Ok(p)
        })
        .transpose()?;

    // ── Affichage fête principale ─────────────────────────────────────────────
    println!("  registry_index : {}", entry.primary_index);
    print_feast(&primary_feast, "  ");
    println!("  secondary      : {} entrée(s) (offset={})",
        entry.secondary_count, entry.secondary_offset);
    println!("  vesperae_i     : {}", entry.has_vesperae_i());
    println!("  vigilia        : {}", entry.has_vigilia());

    if let Some(ref p) = provider {
        print_label(p.get(primary_feast.feast_id, args.year), "  ");
    }

    // ── Fêtes secondaires ─────────────────────────────────────────────────────
    if entry.secondary_count == 0 { return Ok(()); }

    let mut sec_indices = vec![0u16; entry.secondary_count as usize];
    let rc = unsafe {
        kal_read_secondary(
            ptr, len,
            entry.secondary_offset,
            entry.secondary_count,
            sec_indices.as_mut_ptr(),
            entry.secondary_count,
        )
    };
    if rc != KAL_ENGINE_OK {
        return Err(format!("kal_read_secondary : code={rc}"));
    }

    println!();
    println!("  Célébrations secondaires :");

    for (i, &ridx) in sec_indices.iter().enumerate() {
        let mut sf = FeastEntry::zeroed();
        let rc = unsafe { kal_read_feast(ptr, len, ridx, &mut sf) };
        if rc != KAL_ENGINE_OK {
            return Err(format!("kal_read_feast (secondary index={ridx}) : code={rc}"));
        }

        println!();
        println!("    [{i}] registry_index={ridx}");
        print_feast(&sf, "    ");
        if let Some(ref p) = provider {
            print_label(p.get(sf.feast_id, args.year), "    ");
        }
    }

    Ok(())
}

// ── Helpers d'affichage ───────────────────────────────────────────────────────

/// Affiche les champs d'un `FeastEntry` avec le préfixe d'indentation donné.
fn print_feast(fe: &FeastEntry, indent: &str) {
    println!("{indent}feast_id       : {:#06x}", fe.feast_id);
    println!("{indent}flags          : {:#06x}", fe.flags);
    println!("{indent}precedence     : {}", fmt_precedence(fe.precedence()));
    println!("{indent}color          : {}", fmt_color(fe.color()));
    println!("{indent}period         : {}", fmt_period(fe.liturgical_period()));
    println!("{indent}nature         : {}", fmt_nature(fe.nature()));
    println!("{indent}has_vigil_mass : {}", fe.has_vigil_mass());
}

/// Affiche le label et l'annotation d'une entrée `.lits`.
fn print_label(entry: Option<LitsEntry<'_>>, indent: &str) {
    if let Some(e) = entry {
        println!("{indent}label          : {}", e.label);
        if let Some(ann) = e.annotation {
            println!("{indent}annotation     : {}", ann);
        }
    }
}

fn fmt_precedence(r: Result<Precedence, impl std::fmt::Debug>) -> String {
    match r { Ok(p) => format!("{:?} ({})", p, p as u8), Err(e) => format!("invalide ({e:?})") }
}
fn fmt_color(r: Result<Color, impl std::fmt::Debug>) -> String {
    match r { Ok(c) => format!("{:?} ({})", c, c as u8), Err(e) => format!("invalide ({e:?})") }
}
fn fmt_period(r: Result<LiturgicalPeriod, impl std::fmt::Debug>) -> String {
    match r { Ok(p) => format!("{:?} ({})", p, p as u8), Err(e) => format!("invalide ({e:?})") }
}
fn fmt_nature(r: Result<Nature, impl std::fmt::Debug>) -> String {
    match r { Ok(n) => format!("{:?} ({})", n, n as u8), Err(e) => format!("invalide ({e:?})") }
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
