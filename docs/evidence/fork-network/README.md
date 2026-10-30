# Evidenz: Fork-Netzwerk-Erfassung

Dieser Ordner enthält den reproduzierbaren Beleg für die Erfassung zweier
Fork-Netzwerke über die öffentliche GitHub-REST-API. Erstellt am 2026-10-30 im
Auftrag des Agents "Fork Graph Researcher".

## Dateien

| Datei | Inhalt |
| --- | --- |
| `harvest.py` | der Harness. Liest `GET /repos/...` und die zugehörigen Leseendpunkte, schreibt Rohantworten und eine `graph.json`, pausiert bei Rate-Limit bis zum Reset |
| `capture-2026-10-30.json` | das zusammengefasste Ergebnis beider Läufe: Knoten, Kanten, Releases, Commit-Kopf, Upstream-Vergleiche, Endpunkt-Statusliste |

Die Rohantworten der API sind nicht eingecheckt, weil sie mehrere Megabyte
groß sind. Der Harness schreibt sie in seinen Ausgabeordner und ist mit demselben
Aufruf wiederholbar:

```text
python3 docs/evidence/fork-network/harvest.py \
  --out /tmp/forkradar-capture \
  --target chmln/sd \
  --target Semmle/SecurityExploits \
  --sample-size 6 --compare-size 6
```

Der Lauf ist unauthentifiziert und damit auf 60 Anfragen je Stunde begrenzt. Der
genannte Aufruf braucht etwa 14 neue Anfragen, wenn keine Rohantwort vorliegt.

## Bezug

Die inhaltliche Auswertung steht in `docs/fork-network-capture.md`. Die dort
genannten Zahlen stammen aus `capture-2026-10-30.json`.
