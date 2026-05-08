/**
 * app.js — Bridge JS pour liturgical-calendar-wasm.
 *
 * Routes :
 *   /YYYY         → vue annuelle (tableau 366 jours)
 *   /YYYY/MM/DD   → vue journalière (détail complet)
 *   /             → vue journalière, date du jour
 *
 * Mêmes routes supportées avec le hash : /#YYYY, /#YYYY/MM/DD
 */

const WASM_URL = "liturgical_calendar_wasm.wasm";
const KALD_URL = "calendar.kald";
const LITS_URL = "calendar.lits";

const KAL_ENGINE_OK             =  0;
const KAL_ERR_BUILD_ID_MISMATCH = -22;

// ── Lookup tables (miroir de types.rs) ───────────────────────────────────────

const PRECEDENCE = [
    "Triduum Sacrum",
    "Sollemnitates Maiores",
    "Sollemnitates Generales",
    "Sollemnitates Propria",
    "Festa Domini",
    "Dominicae per Annum",
    "Festa BMV et Sanctorum Generales",
    "Festa Propria",
    "Feriae Privilegiatae",
    "Memoriae Obligatoriae Generales",
    "Memoriae Obligatoriae Propria",
    "Memoriae ad Libitum",
    "Feriae per Annum",
];

const COLOR = ["Albus", "Rubeus", "Viridis", "Violaceus", "Rosaceus", "Niger"];
const COLOR_CSS = ["albus", "rubeus", "viridis", "violaceus", "rosaceus", "niger"];
const NATURE = ["Sollemnitas", "Festum", "Dominica", "Memoria", "Commemoratio", "Feria"];
const PERIOD = [
    "Tempus Ordinarium", "Tempus Adventus", "Tempus Nativitatis",
    "Tempus Quadragesimae", "Triduum Paschale", "Tempus Paschale", "Dies Sancti",
];

// ── Layout 366 slots/an (Forge) ───────────────────────────────────────────────

const MONTH_OFFSETS = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];
const MONTH_NAMES   = [
    "janvier","février","mars","avril","mai","juin",
    "juillet","août","septembre","octobre","novembre","décembre"
];

function dateToDoy(_year, month, day) {
    return MONTH_OFFSETS[month - 1] + (day - 1);
}

function doyToMonthDay(doy) {
    for (let m = 11; m >= 0; m--) {
        if (doy >= MONTH_OFFSETS[m]) {
            return { month: m + 1, day: doy - MONTH_OFFSETS[m] + 1 };
        }
    }
    return { month: 1, day: 1 };
}

function zeroPad(n) { return String(n).padStart(2, "0"); }

function formatDateLong(year, month, day) {
    return new Date(year, month - 1, day)
        .toLocaleDateString("fr-FR", { weekday: "long", year: "numeric", month: "long", day: "numeric" });
}

// ── Routage ───────────────────────────────────────────────────────────────────

function detectRoute() {
    const raw = (window.location.hash
        ? window.location.hash.replace(/^#\/?/, "")
        : window.location.pathname.replace(/^\//, "")).replace(/\/$/, "");

    const mYear = raw.match(/^(\d{4})$/);
    if (mYear) return { type: "year", year: parseInt(mYear[1]) };

    const mDay = raw.match(/^(\d{4})\/(\d{2})\/(\d{2})$/);
    if (mDay) return {
        type: "day",
        year:  parseInt(mDay[1]),
        month: parseInt(mDay[2]),
        day:   parseInt(mDay[3]),
    };

    const now = new Date();
    return { type: "day", year: now.getFullYear(), month: now.getMonth() + 1, day: now.getDate() };
}

// ── Utilitaires WASM ──────────────────────────────────────────────────────────

function copyToWasm(memory, ptr, buffer) {
    new Uint8Array(memory.buffer, ptr, buffer.byteLength).set(new Uint8Array(buffer));
}

const decoder = new TextDecoder("utf-8");
function wasmStr(memory, ptr, len) {
    return decoder.decode(new Uint8Array(memory.buffer, ptr, len));
}

/** Décode les flags u16 d'une CalendarEntry. */
function decodeFlags(flags) {
    return {
        precedence:    flags & 0x000F,
        color:         (flags >> 4) & 0x000F,
        period:        (flags >> 8) & 0x0007,
        nature:        (flags >> 11) & 0x0007,
        hasVesperaeI:  !!(flags & (1 << 14)),
        hasVigilia:    !!(flags & (1 << 15)),
    };
}

/** Rendu minimal Markdown inline : *texte* → <em>texte</em>. */
function renderMarkdown(text) {
    return text.replace(/\*([^*]+)\*/g, "<em>$1</em>");
}

/** Lit label + annotation pour un feast_id. Retourne { label, annotation } ou null. */
function resolveById(exports, memory, feastId, year) {
    const rc = exports.kal_wasm_get_label_by_id(feastId, year);
    if (rc !== 1) return null;
    const label = wasmStr(memory, exports.kal_wasm_label_ptr(), exports.kal_wasm_label_len());
    const annLen = exports.kal_wasm_annotation_len();
    const annotation = annLen > 0
        ? wasmStr(memory, exports.kal_wasm_annotation_ptr(), annLen)
        : null;
    return { label, annotation };
}

// ── Vue annuelle ──────────────────────────────────────────────────────────────

function renderYear(year, exports, memory) {
    document.title = `Calendarium ${year}`;
    document.getElementById("h1").textContent = "Calendarium Liturgicum";
    document.getElementById("h2").textContent =
        `Calendarium Romanum Generale pro ${year}`;

    const tbody = document.getElementById("cal-body");
    const entryPtr = exports.kal_wasm_entry_ptr();

    for (let doy = 0; doy < 366; doy++) {
        // Lecture entrée
        const rc = exports.kal_wasm_read_day(year, doy);
        if (rc !== KAL_ENGINE_OK) continue;

        const view = new DataView(memory.buffer, entryPtr, 8);
        const primaryId      = view.getUint16(0, true);
        const secondaryIndex = view.getUint16(2, true);
        const flags          = view.getUint16(4, true);
        const secondaryCount = view.getUint8(6);

        // Padding Entry (pas de fête ce jour)
        if (primaryId === 0) continue;

        const { month, day } = doyToMonthDay(doy);
        const { color }      = decodeFlags(flags);
        const href           = `/${year}/${zeroPad(month)}/${zeroPad(day)}`;

        // Cellule fêtes
        let featsHtml = "";

        // Fête principale
        const rcLabel = exports.kal_wasm_get_label(year, doy);
        if (rcLabel === 1) {
            const label = wasmStr(memory, exports.kal_wasm_label_ptr(), exports.kal_wasm_label_len());
            featsHtml += `<p class="color-${COLOR_CSS[color] ?? ""}"><a href="${href}">${label}</a></p>`;
        }

        // Fêtes secondaires
        // kal_read_secondary retourne KAL_ENGINE_OK (0) en cas de succès —
        // pas le compte. On utilise secondaryCount comme borne de boucle.
        if (secondaryCount > 0) {
            const rc = exports.kal_wasm_read_secondary(secondaryIndex, secondaryCount);
            if (rc === KAL_ENGINE_OK) {
                const secView = new DataView(
                    memory.buffer, exports.kal_wasm_secondary_ptr(), secondaryCount * 2
                );
                for (let i = 0; i < secondaryCount; i++) {
                    const secId = secView.getUint16(i * 2, true);
                    if (secId === 0) continue;
                    const res = resolveById(exports, memory, secId, year);
                    if (res) {
                        featsHtml += `<p class="secondary"><a href="${href}">${res.label}</a></p>`;
                    }
                }
            }
        }

        const tr = document.createElement("tr");
        tr.innerHTML = `
            <td class="doy">${doy}</td>
            <td class="date">${zeroPad(day)}/${zeroPad(month)}</td>
            <td class="feasts">${featsHtml}</td>`;
        tbody.appendChild(tr);
    }

    document.getElementById("cal-table").hidden = false;
}

// ── Vue journalière ───────────────────────────────────────────────────────────

function renderDay(year, month, day, exports, memory) {
    const doy = dateToDoy(year, month, day);

    document.title = `${zeroPad(day)}/${zeroPad(month)}/${year}`;
    document.getElementById("h1").textContent = "Calendarium Liturgicum";
    document.getElementById("h2").textContent = formatDateLong(year, month, day);

    const entryPtr = exports.kal_wasm_entry_ptr();
    const rc = exports.kal_wasm_read_day(year, doy);
    if (rc !== KAL_ENGINE_OK) {
        document.getElementById("day-content").textContent = `Erreur lecture entrée : ${rc}`;
        document.getElementById("day-content").hidden = false;
        return;
    }

    const view = new DataView(memory.buffer, entryPtr, 8);
    const primaryId      = view.getUint16(0, true);
    const secondaryIndex = view.getUint16(2, true);
    const flags          = view.getUint16(4, true);
    const secondaryCount = view.getUint8(6);
    const { precedence, color, period, nature, hasVesperaeI, hasVigilia } = decodeFlags(flags);

    if (primaryId === 0) {
        document.getElementById("day-content").textContent = "(aucune fête)";
        document.getElementById("day-content").hidden = false;
        return;
    }

    // Label + annotation fête principale
    exports.kal_wasm_get_label(year, doy);
    const label      = wasmStr(memory, exports.kal_wasm_label_ptr(), exports.kal_wasm_label_len());
    const annLen     = exports.kal_wasm_annotation_len();
    const annotation = annLen > 0
        ? wasmStr(memory, exports.kal_wasm_annotation_ptr(), annLen)
        : null;

    // Construction du détail principal
    let html = `<section class="feast primary color-${COLOR_CSS[color] ?? ""}">
        <h3>${label}</h3>`;
    if (annotation) {
        html += `<p class="annotation">${renderMarkdown(annotation)}</p>`;
    }
    html += `<dl>
        <dt>Feast ID</dt>   <dd>0x${primaryId.toString(16).toUpperCase().padStart(4,"0")}</dd>
        <dt>Précédence</dt> <dd>${PRECEDENCE[precedence] ?? precedence} (${precedence})</dd>
        <dt>Couleur</dt>    <dd>${COLOR[color] ?? color} (${color})</dd>
        <dt>Période</dt>    <dd>${PERIOD[period] ?? period}</dd>
        <dt>Nature</dt>     <dd>${NATURE[nature] ?? nature}</dd>`;
    if (annotation)   html += `<dt>Annotation</dt><dd>${renderMarkdown(annotation)}</dd>`;
    if (hasVesperaeI) html += `<dt>Vêpres I</dt><dd>oui</dd>`;
    if (hasVigilia)   html += `<dt>Vigile</dt>  <dd>oui</dd>`;
    html += `</dl></section>`;

    // Fêtes secondaires
    if (secondaryCount > 0) {
        const rcSec = exports.kal_wasm_read_secondary(secondaryIndex, secondaryCount);
        if (rcSec === KAL_ENGINE_OK) {
            const secView = new DataView(
                memory.buffer, exports.kal_wasm_secondary_ptr(), secondaryCount * 2
            );
            html += `<section class="secondaries"><h4>Commémorations</h4>`;
            for (let i = 0; i < secondaryCount; i++) {
                const secId = secView.getUint16(i * 2, true);
                if (secId === 0) continue;
                const res = resolveById(exports, memory, secId, year);
                if (res) {
                    html += `<div class="feast secondary"><strong>${res.label}</strong>`;
                    if (res.annotation) {
                        html += `<p class="annotation">${renderMarkdown(res.annotation)}</p>`;
                    }
                    html += `</div>`;
                }
            }
            html += `</section>`;
        }
    }

    // Navigation
    const prev = doyToMonthDay(Math.max(0, doy - 1));
    const next = doyToMonthDay(Math.min(365, doy + 1));
    html += `<nav class="day-nav">
        <a href="/${year}/${zeroPad(prev.month)}/${zeroPad(prev.day)}">← Jour précédent</a>
        <a href="/${year}">Année ${year}</a>
        <a href="/${year}/${zeroPad(next.month)}/${zeroPad(next.day)}">Jour suivant →</a>
    </nav>`;

    const container = document.getElementById("day-content");
    container.innerHTML = html;
    container.hidden = false;
}

// ── Initialisation ────────────────────────────────────────────────────────────

async function init() {
    const status = document.getElementById("status");

    try {
        const [wasmResp, kaldBuf, litsBuf] = await Promise.all([
            fetch(WASM_URL),
            fetch(KALD_URL).then(r => r.arrayBuffer()),
            fetch(LITS_URL).then(r => r.arrayBuffer()),
        ]);

        const { instance } = await WebAssembly.instantiateStreaming(wasmResp, {});
        const { exports }  = instance;
        const memory       = exports.memory;

        const kaldPtr = exports.kal_wasm_alloc_kald(kaldBuf.byteLength);
        if (kaldPtr === 0) throw new Error("kald : capacité statique insuffisante");
        copyToWasm(memory, kaldPtr, kaldBuf);
        const rcKald = exports.kal_wasm_commit_kald();
        if (rcKald !== KAL_ENGINE_OK) throw new Error(`kal_wasm_commit_kald → ${rcKald}`);

        const litsPtr = exports.kal_wasm_alloc_lits(litsBuf.byteLength);
        if (litsPtr === 0) throw new Error("lits : capacité statique insuffisante");
        copyToWasm(memory, litsPtr, litsBuf);
        const rcLits = exports.kal_wasm_commit_lits();
        if (rcLits === KAL_ERR_BUILD_ID_MISMATCH)
            throw new Error("build_id mismatch : .kald et .lits issus de builds différents");
        if (rcLits !== KAL_ENGINE_OK)
            throw new Error(`kal_wasm_commit_lits → ${rcLits}`);

        status.hidden = true;

        const route = detectRoute();
        if (route.type === "year") {
            renderYear(route.year, exports, memory);
        } else {
            renderDay(route.year, route.month, route.day, exports, memory);
        }

    } catch (err) {
        status.textContent = `Erreur : ${err.message}`;
        status.hidden = false;
        console.error(err);
    }
}

init();
