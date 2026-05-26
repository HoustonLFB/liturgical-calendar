#!/usr/bin/env python3
"""
server.py — Serveur de développement local pour liturgical-calendar-wasm.

Réécriture SPA : toute requête vers un chemin non-fichier est servie par
index.html. Permet le routage par chemin (/2026/12/25) sans configuration
supplémentaire.

Usage :
    python3 server.py              # port 8080 par défaut
    python3 server.py 9000         # port personnalisé
"""

import os
import sys
from http.server import HTTPServer, SimpleHTTPRequestHandler


WWW_DIR = os.path.dirname(os.path.abspath(__file__))


class SpaHandler(SimpleHTTPRequestHandler):
    # Invariant réseau : Forcer les types MIME requis pour le pipeline WASM / DOD
    extensions_map = SimpleHTTPRequestHandler.extensions_map.copy()
    extensions_map.update({
        ".wasm": "application/wasm",
        ".kald": "application/octet-stream",
        ".lits": "application/octet-stream",
    })

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=WWW_DIR, **kwargs)

    def do_GET(self):
        # Isolation du chemin propre sans la query string
        path_clean = self.path.split("?")[0]

        # Si on demande la racine, on sert la version locale dédiée
        if path_clean in ("/", "/index.html"):
            self.path = "/index_local.html"
            super().do_GET()
            return

        # Résolution du chemin physique
        candidate = os.path.join(WWW_DIR, path_clean.lstrip("/"))

        # Routage SPA : si la ressource n'existe pas ou est un dossier
        if not os.path.exists(candidate) or os.path.isdir(candidate):
            # Filtre strict : Ne jamais servir index_local.html pour un asset technique manquant
            ext = os.path.splitext(path_clean)[1]
            if ext not in (".wasm", ".kald", ".lits", ".js", ".css", ".png", ".ico", ".woff2"):
                self.path = "/index_local.html"
        
        super().do_GET()

    def log_message(self, fmt, *args):
        # Silence les logs de ressources statiques (.wasm, .kald, .lits, .js).
        if any(self.path.endswith(ext) for ext in (".wasm", ".kald", ".lits", ".js", ".css")):
            return
        super().log_message(fmt, *args)


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
    server = HTTPServer(("0.0.0.0", port), SpaHandler)
    print(f"http://0.0.0.0:{port}/")
    print(f"Exemples :")
    print(f"  http://0.0.0.0:{port}/             → date du jour")
    print(f"  http://0.0.0.0:{port}/2026/12/25   → chemin (réécriture SPA active)")
    print(f"  http://0.0.0.0:{port}/2026         → année en cours")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nArrêt.")
