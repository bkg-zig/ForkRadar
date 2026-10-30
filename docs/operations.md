# Betrieb, Werkzeuge und Prüfkommandos

Stand 2026-10-29, erstellt in Task FR-AUDIT-001. Alle Kommandos wurden in der angegebenen Form ausgeführt, sofern nicht ausdrücklich als NOT_RUN markiert.

## Git-Zustand

- Branch `main` mit Upstream `origin/main`, HEAD `e093496`.
- Arbeitsbaum war zum Auditzeitpunkt sauber. Neu und untracked sind `.brain/` und `docs/` aus diesem Audit.
- Remote `origin` zeigt auf `https://github.com/bkg-zig/ForkRadar`. Das Remote ist erreichbar, `git ls-remote --heads origin` liefert `e0934964446364596b56a336da2ea2605406344e` für `refs/heads/main`.
- Keine Tags, kein Stash, keine weiteren lokalen Branches, kein Upstream-Divergenz zwischen `main` und `origin/main`.
- Die Git-Metadaten liegen seit 19:52 in `.gsp/`. Die Datei `.git` ist ein Zeiger mit dem Inhalt `gitdir: .gsp`. `git rev-parse --git-dir` liefert `/home/openchamber/dump/coding/ForkRadar/.gsp`. `.gsp/` ist über die Ausschlussliste `.gsp/info/exclude` ausgeschlossen und erscheint deshalb nicht in `git status`. Die Historie in `.gsp` umfasst dieselben drei Commits.

### Zugriffshinweis für Git

Die Repository-Dateien gehören der UID 1000, die Shell läuft als root. Git verweigert Operationen deshalb mit "dubious ownership in repository at '/home/openchamber/dump/coding/ForkRadar'". In dieser Umgebung wurde jedes Kommando mit dem Zusatz ausgeführt:

```text
git -c safe.directory=/home/openchamber/dump/coding/ForkRadar <kommando>
```

Die globale Git-Konfiguration wurde bewusst nicht verändert. Eine dauerhafte Lösung ist eine Eigentümerentscheidung, siehe FR-018.

Die Ablage der Metadaten in `.gsp/` stammt aus einer parallelen Sitzung. Wer committet, arbeitet damit gegen dieses Verzeichnis. Die Hook-Dateien in `.gsp/hooks/` sind die mitgelieferten Beispieldateien, kein aktivierter Hakenlauf.

## Werkzeugbestand

| Werkzeug | Status | Beleg |
| --- | --- | --- |
| `cargo` | fehlt | `command -v cargo` ohne Treffer |
| `rustc` | fehlt | `command -v rustc` ohne Treffer |
| `rustup` | fehlt | `command -v rustup` ohne Treffer |
| `dx` (Dioxus CLI) | fehlt | `command -v dx` ohne Treffer |
| `node` | vorhanden, v20.19.2 | `node --version` |
| `npm` | vorhanden | `command -v npm` |
| `python3` | vorhanden, 3.13.5 | `python3 --version` |
| `docker` | vorhanden, Daemon 29.9.2 | `docker info --format '{{.ServerVersion}}'` |
| `git` | vorhanden, 2.47.3 | `git --version` |
| `psql`, `postgres`, `sqlite3` | fehlen | `command -v` ohne Treffer |
| `go`, `gh` | fehlen | `command -v` ohne Treffer |

## Prüfkommandos für den späteren Workspace

Das README nennt in Abschnitt 19 die folgenden Kommandos als Release-Gate. Sie sind NOT_RUN, weil kein Workspace existiert.

```text
cargo fmt --check
cargo clippy
cargo test
dx build --platform web
```

Ein erfolgreicher Build belegt keine funktionale Korrektheit. Nicht durchgeführte Prüfungen werden als NOT_RUN gemeldet.

## NIM-Diagnosepfad

Der README-Abschnitt 17 beschreibt eine serverseitige Diagnose, die die Basis-URL validiert, die Erreichbarkeit prüft, die authentifizierte Modellliste abfragt, das konfigurierte Modell gegen die Liste abgleicht und einen minimalen Inferenztest ausführt. Bisher belegt ist ein Teilschritt.

```text
GET https://nim.eysho.info/v1/models  ->  HTTP 401
{"error":{"code":"unauthorized","message":"missing or invalid proxy API key (Authorization: Bearer ...)","type":"proxy_error"}}
```

Die Antwort kommt vom Proxy der Installation und nennt einen fehlenden Bearer-Key. Damit sind der Pfad `/v1` und die Basis-URL `https://nim.eysho.info/v1` bestätigt, die Authentifizierung ist offen. Status nach README-Notation: `NIM_NOT_CONFIGURED`. Ein erfolgreicher Abruf der Modellliste allein belegt keine Chat-Completions-, Tool-Calling-, Streaming- oder JSON-Fähigkeit.

## Infrastruktur

- Der Docker-Daemon läuft. Im Repository liegen weder `Dockerfile` noch `docker-compose.yml`, es gibt kein benanntes Deployment-Ziel. ERFORDERT ANALYSE.
- Es ist keine Datenbank erreichbar, weder lokal noch laut Konfiguration. Damit fehlt die Grundlage für das persistente Job-Modell aus README-Abschnitt 6. ERFORDERT ANALYSE.
- Es läuft kein Scheduler. Die wiederkehrende Discovery aus README-Abschnitt 5.5 ist damit nicht eingerichtet und darf nicht als aktiv angezeigt werden.
- Es gibt keine CI-Konfiguration. Der `.github`-Ordner fehlt.

## BKG-Werkzeuge im Projekt

| Werkzeug | Ergebnis |
| --- | --- |
| `bkg-git-team status` | `## main...origin/main` und `?? .brain/` |
| `bkg-supervisor-team` | alle Listen leer, `config: null`, also keine projektlokale Agenten-, Skill-, Tool- oder Regelkonfiguration |
| `bkg-list-goal` | "Keine Goals gefunden" |
| `bkg-list-plan` | 0 Pläne |
| `.brain/teams.json` | fünf Teams mit je vier Agenten. Zwei mit Status `planned` und ohne Skills, drei mit Status `started` und mit Skills. Das letzte Team entstand um 20:02:37Z. |

## Evidenz

```text
$ git -c safe.directory=/home/openchamber/dump/coding/ForkRadar status --porcelain=v1 -b
## main...origin/main
?? .brain/

$ git -c safe.directory=/home/openchamber/dump/coding/ForkRadar branch -a -vv
* main                e093496 [origin/main] Revise architecture guidelines for ForkRadar project
  remotes/origin/HEAD -> origin/main
  remotes/origin/main e093496 Revise architecture guidelines for ForkRadar project

$ git -c safe.directory=/home/openchamber/dump/coding/ForkRadar tag -l
<leer>
```
