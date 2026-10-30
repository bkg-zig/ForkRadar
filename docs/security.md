# Sicherheitsgrenzen und Konfigurationsstatus

Stand 2026-10-29, erstellt in Task FR-AUDIT-001. Der Stand ist belegt, nicht angenommen.

## Aktueller Schutzstand

Es läuft kein Anwendungscode. Es gibt keine Authentifizierung, keine Benutzerisolation, keine Datenbank, keine Logging-Pipeline und keinen Deployment. Jede Aussage über vorhandene Schutzmechanismen wäre irreführend. Dieses Dokument hält fest, welche Grenzen das README verlangt und welche Konfiguration tatsächlich vorhanden ist.

## Geforderte Grenzen laut README

- GitHub-OAuth läuft serverseitig im Axum-Backend, mit geprüftem `state`, mit PKCE, soweit der gewählte Flow es unterstützt, und mit Session-Cookie mit `HttpOnly`, `Secure` in HTTPS-Produktion und geeignetem `SameSite` (Abschnitt 7.2).
- Benutzerisolation wird serverseitig geprüft. Frontend-Guards gelten nicht als Schutz. Datenbank-Constraints oder Policies kommen zusätzlich zur Prüfung bei jeder geschützten Operation (Abschnitt 8).
- Der BKG-NIM-Key bleibt serverseitig. Er darf weder im Browser-Code noch in Frontend-Umgebungsvariablen mit öffentlichem Präfix, Browser-Storage, Logs, Exceptions, Reports, URLs, Git-Diffs oder Testausgaben landen (Abschnitte 2.2 und 17).
- Es gibt kein Google-spezifisches Backend und kein Fallback auf einen externen LLM-Anbieter (Abschnitt 2.2).
- Analysierter Repository-Code gilt als nicht vertrauenswürdige Daten. Er darf keine Systemanweisungen, Berechtigungen oder Tool-Aufrufe festlegen (Abschnitt 17).
- Fremder Repository-Code wird nicht unkontrolliert ausgeführt. Statische Analyse erfolgt soweit möglich in isolierten Workern mit begrenzten Ressourcen (Abschnitt 1.4).
- Zerstörerische Operationen, Datenlöschungen und Berechtigungsänderungen brauchen eine ausdrückliche Freigabe (Abschnitt 1.2).

## Konfigurationsstatus

| Variable | Zweck | Status | Beleg |
| --- | --- | --- | --- |
| `GITHUB_OAUTH_CLIENT_ID` | OAuth-Client-ID des GitHub-Apps | NOT_CONFIGURED | keine Umgebungsvariable mit Präfix `GITHUB_` vorhanden |
| `GITHUB_OAUTH_CLIENT_SECRET` | OAuth-Client-Secret | NOT_CONFIGURED | dieselbe Prüfung |
| `GITHUB_OAUTH_CALLBACK_URL` | Callback-URL | NOT_CONFIGURED | dieselbe Prüfung |
| `GITHUB_API_TOKEN` | optionaler API-Token | NOT_CONFIGURED | dieselbe Prüfung |
| `BKG_NIM_BASE_URL` | Basis-URL des internen Modell-Backends | Standardwert im README, Laufzeitwert unbekannt | ERFORDERT ANALYSE |
| `BKG_NIM_API_KEY` | serverseitiger API-Key | NOT_CONFIGURED | HTTP 401 bei `GET https://nim.eysho.info/v1/models` |
| `BKG_NIM_MODEL` | Modellname | unbekannt | ERFORDERT ANALYSE, das README verbietet erfundene Modellnamen |
| Datenbank-URL | Verbindung zur Produktionsdatenbank | unbekannt | ERFORDERT ANALYSE, keine Datenbankwerkzeuge vorhanden |

## Secret-Handling im Repository

- Es existiert keine `.env` und keine `.env.example`. Damit gibt es auch keine Datei, in der versehentlich ein funktionierender Zugangswert stehen könnte.
- Die Prüfung der Umgebungsvariablen ergab keine Namen mit den Präfixen `BKG_`, `GITHUB_`, `NIM_`, `DATABASE_URL` oder `POSTGRES`. Es wurde ausschließlich nach Namen gefiltert, Werte wurden nicht ausgegeben.
- Die Git-Historie enthält drei Commits, die ausschließlich `README.md` verändern. Es wurden keine Credentials eingetragen.

## Berechtigungen und Umfeld

- Der Docker-Daemon ist erreichbar und meldet Version 29.9.2. Ein Container-Build ist damit technisch möglich, ein Zielsystem ist nicht benannt.
- Der Zugriff auf das Git-Remote funktioniert. `git ls-remote --heads origin` liefert `e0934964446364596b56a336da2ea2605406344e` für `refs/heads/main`.
- Die Repository-Dateien gehören der UID 1000, während die Shell als root läuft. Git verweigert deshalb Operationen mit "dubious ownership". Die Umgehung per `git -c safe.directory=...` ist in `docs/operations.md` dokumentiert. Eine globale Git-Konfiguration wurde nicht verändert.

## Offene Sicherheitsaufgaben

- FR-012 GitHub-OAuth-Bereitstellung. Ohne Client-ID, Secret und Callback-URL ist der Anmeldefluss nicht konfigurierbar. Status NOT_CONFIGURED.
- FR-013 BKG-NIM-Modell und Fähigkeiten verifizieren. Der Endpunkt ist erreichbar, ein Key fehlt. Der Modellname ist erst nach erfolgreicher Autorisierung ermittelbar.
- FR-020 Analyse fremder Repositories isolieren. Ohne Code ist keine Ausführungsgrenze vorhanden. Das betrifft auch die Pflicht, Installationsskripte und Package-Scripts nicht automatisch zu starten.
- FR-021 Prompt-Injection-Abwehr für Quelltextdaten. Ohne NIM-Adapter nicht umsetzbar, aber vor der ersten KI-Funktion zu bauen.
- ERFORDERT ANALYSE: Ob das Repository öffentlich sichtbar ist, wurde nicht geprüft. Das README nennt interne Hostnamen und eine Zielumgebung. Bei öffentlicher Sichtbarkeit wäre zu prüfen, ob der Inhalt vor der Veröffentlichung bereinigt werden muss.

## Evidenz

```text
$ env | cut -d= -f1 | grep -E '^(BKG_|GITHUB_|NIM_|DATABASE_URL|POSTGRES)' | sort
<keine Ausgabe>

$ curl -sS -o /tmp/opencode/nim_models.txt -w 'http_code=%{http_code} time=%{time_total}\n' --max-time 15 https://nim.eysho.info/v1/models
http_code=401 time=0.274769
{"error":{"code":"unauthorized","message":"missing or invalid proxy API key (Authorization: Bearer ...)","type":"proxy_error"}}

$ timeout 10 docker info --format '{{.ServerVersion}}'
29.9.2
```
