# Entscheidungsprotokolle

Stand 2026-10-29, angelegt in Task FR-AUDIT-002.

## Zweck

Dieser Ordner sammelt die Debatten und Entscheidungsprotokolle des Projekts. Die globale Regel `04-debate-voting` verlangt für Planungs-, Architektur-, Feature- und Risikoentscheidungen drei getrennte Perspektiven mit sichtbarer Abstimmung. README-Abschnitt 18 verlangt dieselbe Prüfung aus drei Blickwinkeln:

1. Architektur und Wartbarkeit
2. Sicherheit und Datenintegrität
3. Produktanforderungen und Benutzerfreundlichkeit

Ohne echte Multi-Agent-Unterstützung sind das Reviews aus getrennten Perspektiven. Sie werden nicht als Abstimmung realer Agenten dargestellt.

## Aktueller Stand

Es liegt kein Debattenprotokoll vor. Die prüfpflichtige Architekturentscheidung ist der Zielstack Rust mit Dioxus und Axum aus README-Abschnitt 2.1. Sie ist noch nicht bewertet, weil im aktuellen Environment keine Rust-Toolchain verfügbar ist und damit kein prüfbarer Stand existiert. Der Punkt ist als FR-016 in `.brain/tasks.json` erfasst.

## Vorgeschlagenes Format

Für jede Debatte eine Datei mit dem Namen `JJJJ-MM-TT-kurzes-thema.md`. Vorgeschlagene Abschnitte:

1. Anlass und betroffene Entscheidung
2. Belege und Ausgangslage mit Kommandos und Ausgaben
3. Position aus Perspektive Architektur und Wartbarkeit
4. Position aus Perspektive Sicherheit und Datenintegrität
5. Position aus Perspektive Produktanforderungen und Benutzerfreundlichkeit
6. Gegenargumente und offene Zielkonflikte
7. Abstimmung mit Stimmenzahl und abweichenden Positionen
8. Entscheidung, Begründung und Auswirkung
9. Verbleibende Risiken und Folgeaufgaben

Geplantes gilt nicht als abgeschlossen. Eine Entscheidung ist erst dokumentiert, wenn die betroffene Änderung umgesetzt und geprüft ist.
