# Architektur und Projektbestand

Stand 2026-10-29, erstellt in Task FR-AUDIT-001. Jede Zustandsaussage stammt aus einem ausgeführten Kommando. Die Kommandos stehen im Abschnitt "Evidenz".

## Ist-Zustand

ForkRadar ist derzeit ein Anforderungsdokument und keine laufende Anwendung. Das Repository enthält genau eine versionierte Datei: `README.md` mit 862 Zeilen und 48.602 Bytes. Das Dokument trägt den Titel "FORKRADAR — MASTER IMPLEMENTATION PROMPT" und gliedert sich in 22 nummerierte Abschnitte von "AUFTRAG" bis "JETZT AUSFÜHREN".

Anwendungscode, Build-Konfiguration, Tests, CI und Projektdokumentation fehlten zum Auditzeitpunkt vollständig. Die Git-Historie besteht aus drei Commits vom 29.10.2026. Alle drei verändern ausschließlich `README.md`. Der Commit 973ad6c trägt die Nachricht "Update fmt.Println message from 'Hello' to 'Goodbye'", betrifft aber keinen Go-Code. In diesem Repository hat es nie Quellcode gegeben.

Der Zielaufbau aus dem README ist damit Planung, nicht Stand.

## Vorgesehener Zielaufbau laut README

Das README schreibt den Stack vor, in Abschnitt 2.1 und in der Reihenfolge aus Abschnitt 21: Rust mit Dioxus 0.7.x für das Web-Frontend, Rust mit Axum für das Backend, gemeinsame Crates für Domain-Modelle, GitHub-Client, Persistenz, statische Analyse und NIM-Adapter. Dioxus 0.8 Alpha ist ausgeschlossen. Als beispielhafte Workspace-Struktur nennt das README `apps/forkradar-web`, `apps/forkradar-api` und getrennte Crates. PostgreSQL ist als Startpunkt für relationale Daten und langlebige Job-Queues genannt, SQLite nur bei nachweislich passendem Deployment, Sperrverhalten und Backup.

BKG-NIM ist laut Abschnitt 2.2 das einzige Modell-Backend, erreichbar unter `https://nim.eysho.info/v1`. Google-spezifische Dienste sind ausgeschlossen. Die Abschnitte 5 und 6 definieren Discovery ohne festes Standard-Tiefenlimit, persistente Jobs, Checkpoints und Wiederaufnahme. Abschnitt 8 listet 21 logische Datenbereiche von `users` bis `audit_events`. Abschnitt 18 fordert genau diese Dokumentation und genau die Brain-Dateien, die dieses Audit angelegt hat.

Diese Vorgaben sind Spezifikation. Ob sie gegen eine vorhandene BKG-Infrastruktur konfliktfrei umsetzbar sind, ist nicht belegt.

## Belegter Stand je Bereich

| Bereich | Belegter Stand | Beleg |
| --- | --- | --- |
| Anwendungscode | nicht vorhanden | `git ls-files` liefert nur `README.md` |
| Build- und Paketkonfiguration | nicht vorhanden | `Cargo.toml`, `Cargo.lock`, `package.json`, `pyproject.toml`, `go.mod`, `Makefile`, `justfile`, `rust-toolchain.toml` fehlen |
| CI | nicht vorhanden | `.github/` fehlt, keine Workflow-Datei |
| Tests | nicht vorhanden | keine Testdatei und kein Testkommando im Repository |
| Dokumentation | nur `README.md` | `docs/` fehlte zum Auditzeitpunkt und wurde in FR-AUDIT-002 angelegt |
| Brain | zum Auditzeitpunkt nur `teams.json` vorhanden | `bkg-list-goal` meldet keine Goals, `bkg-list-plan` meldet 0 Pläne. Das Audit hat `project.json`, `tasks.json`, `decisions.json`, `blockers.json` und `memory.json` ergänzt. |
| Projektlokale BKG-Konfiguration | nicht vorhanden | `bkg-supervisor-team` meldet leere Agents, Skills, Tools, Regeln und `config: null` |
| Toolchain Rust | nicht vorhanden | `cargo`, `rustc`, `rustup` und `dx` fehlen im PATH |
| Toolchain sonst | Node 20.19.2, npm, Python 3.13.5, Docker 29.9.2 und Git 2.47.3 vorhanden | `command -v` und `docker info` |
| GitHub-OAuth | nicht konfiguriert | keine Umgebungsvariable mit dem Präfix `GITHUB_` vorhanden |
| BKG-NIM | erreichbar, nicht autorisiert | `GET https://nim.eysho.info/v1/models` liefert HTTP 401 |
| Datenbank | keine vorhanden | `psql`, `postgres` und `sqlite3` fehlen, keine Compose-Datei |

## Offene Punkte

- ERFORDERT ANALYSE: Rust-Toolchain und Dioxus-CLI sind im aktuellen Environment nicht vorhanden. Ohne sie ist der Zielstack nicht baubar. Ob eine BKG-Toolchain an anderer Stelle bereitsteht, ist zu klären.
- ERFORDERT ANALYSE: Ob die vorgesehene Workspace-Struktur mit einer realen BKG-Umgebung kollidiert, ist nicht geprüft. Im Repository liegt kein weiterer BKG-Workspace.
- ERFORDERT ANALYSE: Datenmodell und Migrationen gegen eine reale BKG-Datenbank sind ungeprüft.
- ERFORDERT ANALYSE: `.brain/teams.json` führt nach parallelen Sitzungen fünf Teams mit je vier Agenten. Zwei Teams stehen auf `planned`, drei auf `started`. Die dort genannten Modellnamen sind gegen keine tatsächliche Konfiguration geprüft, und die drei gestarteten Teams tragen Skills, die keine projektlokale Agentenkonfiguration im Sinne des Supervisors belegen.
- ERFORDERT ANALYSE: Ob das Repository `github.com/bkg-zig/ForkRadar` öffentlich sichtbar ist, wurde nicht geprüft. Das entscheidet über die Frage, ob der Prompt selbst sensible Zielinformationen trägt.
- Die Lizenz ist nicht festgelegt. `LICENSE` fehlt, die Festlegung ist eine Eigentümerentscheidung.

## Evidenz

```text
$ git -c safe.directory=/home/openchamber/dump/coding/ForkRadar log --format='%h|%ad|%an|%s' --date=iso --name-status
ac48a57|2026-10-29 19:34:00 +0100|bkg.zig|Initial commit
A	README.md
973ad6c|2026-10-29 19:34:51 +0100|bkg.zig|Update fmt.Println message from 'Hello' to 'Goodbye'
M	README.md
e093496|2026-10-29 19:38:35 +0100|bkg.zig|Revise architecture guidelines for ForkRadar project
M	README.md

$ git ls-files
README.md

$ curl -sS -o /tmp/opencode/nim_models.txt -w 'http_code=%{http_code} time=%{time_total}\n' --max-time 15 https://nim.eysho.info/v1/models
http_code=401 time=0.274769
{"error":{"code":"unauthorized","message":"missing or invalid proxy API key (Authorization: Bearer ...)","type":"proxy_error"}}

$ bkg-supervisor-team
{"agents":[],"commands":[],"skills":[],"tools":[],"rules":[],"plugins":[],"config":null}
```

Belege zu Werkzeugbestand, Umgebungsvariablen, Datenbank und Git-Status stehen in `docs/operations.md`.
