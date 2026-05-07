/**
 * app.js — Bridge JS pour liturgical-calendar-wasm.
 *
 * Routage :
 *   /#2026/12/25       → hash (fonctionne sur tout hébergeur statique)
 *   /2026/12/25        → chemin (requiert réécriture SPA côté serveur)
 *   /                  → date du jour
 */

const WASM_URL = "liturgical_calendar_wasm.wasm";
const KALD_URL = "calendar.kald";
const LITS_URL = "calendar.lits";

const KAL_ENGINE_OK            =  0;
const KAL_ERR_BUILD_ID_MISMATCH = -22;

// ── Routage ───────────────────────────────────────────────────────────────────

/**
 * Résout la date cible depuis l'URL.
 *
 * Priorité : hash (#YYYY/MM/DD) > chemin (/YYYY/MM/DD) > date du jour.
 * Retourne { year, month, day } ou null si l'URL ne contient pas de date valide.
 */
function parseDateFromUrl() {
    const raw = window.location.hash
        ? window.location.hash.replace(/^#\/?/, "")
        : window.location.pathname.replace(/^\//, "");

    const match = raw.match(/^(\d{4})\/(\d{2})\/(\d{2})$/);
    if (!match) return null;

    const year  = parseInt(match[1], 10);
    const month = parseInt(match[2], 10);
    const day   = parseInt(match[3], 10);

    if (month < 1 || month > 12 || day < 1 || day > 31) return null;
    return { year, month, day };
}

/**
 * Convertit (month, day) en doy 0-based selon le layout fixe 366 slots/an de la Forge.
 *
 * La Forge réserve toujours 366 créneaux par année : le slot 59 est celui du
 * 29 février (Padding Entry si année non bissextile). Tous les jours à partir
 * du 1er mars ont donc un doy fixe, indépendant de la bissextilité.
 * Le paramètre year n'intervient pas dans ce calcul — il est conservé en
 * signature pour être passé à kal_wasm_get_label (résolution .lits).
 *
 * Vérification : 25 décembre → MONTH_OFFSETS[11] + 24 = 335 + 24 = 359.
 */
const MONTH_OFFSETS = [
//  Jan  Fév  Mar  Avr  Mai  Jun  Jul  Aoû  Sep  Oct  Nov  Déc
      0,  31,  60,  91, 121, 152, 182, 213, 244, 274, 305, 335
];

function dateToDoy(_year, month, day) {
    return MONTH_OFFSETS[month - 1] + (day - 1);
}

/** Formatte une date en français sans dépendance externe. */
function formatDate(year, month, day) {
    return new Date(year, month - 1, day)
        .toLocaleDateString("fr-FR", { weekday: "long", year: "numeric", month: "long", day: "numeric" });
}

// ── Utilitaires WASM ──────────────────────────────────────────────────────────

function copyToWasm(memory, ptr, buffer) {
    new Uint8Array(memory.buffer, ptr, buffer.byteLength)
        .set(new Uint8Array(buffer));
}

const decoder = new TextDecoder("utf-8");
function wasmString(memory, ptr, len) {
    return decoder.decode(new Uint8Array(memory.buffer, ptr, len));
}

// ── Initialisation ────────────────────────────────────────────────────────────

async function init() {
    const status = document.getElementById("status");
    const feast  = document.getElementById("feast");

    try {
        const [wasmResp, kaldBuf, litsBuf] = await Promise.all([
            fetch(WASM_URL),
            fetch(KALD_URL).then(r => r.arrayBuffer()),
            fetch(LITS_URL).then(r => r.arrayBuffer()),
        ]);

        const { instance } = await WebAssembly.instantiateStreaming(wasmResp, {});
        const { exports }  = instance;
        const memory       = exports.memory;

        // — Commit .kald —
        const kaldPtr = exports.kal_wasm_alloc_kald(kaldBuf.byteLength);
        if (kaldPtr === 0) throw new Error("kald : buffer trop grand pour la capacité statique");
        copyToWasm(memory, kaldPtr, kaldBuf);
        const rcKald = exports.kal_wasm_commit_kald();
        if (rcKald !== KAL_ENGINE_OK) throw new Error(`kal_wasm_commit_kald → ${rcKald}`);

        // — Commit .lits —
        const litsPtr = exports.kal_wasm_alloc_lits(litsBuf.byteLength);
        if (litsPtr === 0) throw new Error("lits : buffer trop grand pour la capacité statique");
        copyToWasm(memory, litsPtr, litsBuf);
        const rcLits = exports.kal_wasm_commit_lits();
        if (rcLits === KAL_ERR_BUILD_ID_MISMATCH)
            throw new Error("build_id mismatch : .kald et .lits issus de builds différents");
        if (rcLits !== KAL_ENGINE_OK)
            throw new Error(`kal_wasm_commit_lits → ${rcLits}`);

        // — Résolution de la date —
        const parsed = parseDateFromUrl();
        const { year, month, day } = parsed ?? (() => {
            const now = new Date();
            return { year: now.getFullYear(), month: now.getMonth() + 1, day: now.getDate() };
        })();
        const doy = dateToDoy(year, month, day);

        // — Label —
        const rc = exports.kal_wasm_get_label(year, doy);
        status.textContent = formatDate(year, month, day);

        if (rc === 1) {
            const ptr = exports.kal_wasm_label_ptr();
            const len = exports.kal_wasm_label_len();
            feast.textContent = wasmString(memory, ptr, len);
        } else if (rc === 0) {
            feast.textContent = "(aucune fête)";
        } else {
            throw new Error(`kal_wasm_get_label → ${rc}`);
        }

    } catch (err) {
        status.textContent = `Erreur : ${err.message}`;
        console.error(err);
    }
}

init();
