# FrigatePiP — Benutzerhandbuch

**Languages:** [English](../en/user-guide.md) · [Polski](../pl/user-guide.md) · [Deutsch](../de/user-guide.md) · [Español](../es/user-guide.md) · [Português](../pt/user-guide.md) · [Français](../fr/user-guide.md)

FrigatePiP ist eine kleine Windows-Anwendung im Infobereich der Taskleiste für [Frigate NVR](https://frigate.video). Erkennt Frigate eine Person, ein Auto oder ein anderes Objekt, zeigt FrigatePiP sofort in einer Bildschirmecke ein kleines Bild-in-Bild-Fenster mit dem Livevideo dieser Kamera. Bewegt sich nichts mehr, schließt sich das Fenster von selbst.

Die Abschnitte 1–11 beschreiben die Windows-Anwendung. FrigatePiP läuft auch unter Linux — auf dem Desktop und auf einer Konsole ohne Desktop; diese Version wird gesondert in Abschnitt 12 beschrieben, [Linux-Version](#12-linux-version).

## Inhalt

1. [Voraussetzungen](#1-voraussetzungen)
2. [Installation](#2-installation)
3. [Erster Start](#3-erster-start)
4. [Das Tray-Symbol](#4-das-tray-symbol)
5. [Vorschaufenster](#5-vorschaufenster)
6. [Das Einstellungsfenster](#6-das-einstellungsfenster)
7. [Die Einstellungsdatei](#7-die-einstellungsdatei)
8. [Befehlszeile](#8-befehlszeile)
9. [Sprache](#9-sprache)
10. [Dateien, Datenschutz und Ressourcen](#10-dateien-datenschutz-und-ressourcen)
11. [Fehlerbehebung](#11-fehlerbehebung)
12. [Linux-Version](#12-linux-version)

---

## 1. Voraussetzungen

- **Windows 10 (Version 1809 oder neuer) oder Windows 11**, 64 Bit.
- **Frigate** muss vom PC aus über das Netzwerk erreichbar sein. Empfohlen wird Frigate 0.14 oder neuer; die *Review*-Modi erfordern 0.14 oder neuer.
- Die **API** von Frigate muss vom PC aus erreichbar sein: der nicht authentifizierte **Port 5000** (`http://`, ohne Anmeldung) oder der authentifizierte **Port 8971** (`https://`, mit einem Frigate-Benutzer – die Rolle *viewer* genügt; anlegen in Frigate unter *Einstellungen → Benutzer*). Unter Docker veröffentlichen Sie den Port, z. B. `- "5000:5000"` oder `- "8971:8971"`. Ein Reverse-Proxy vor Frigate funktioniert ebenfalls.
- **Frigate-Add-on für Home Assistant:** Port 5000 ist standardmäßig deaktiviert – aktivieren Sie ihn in den *Netzwerk*-Einstellungen des Add-ons. Alternativ verwenden Sie Port 8971 mit einem Frigate-Benutzer.
- Für flüssiges Livevideo: der in Frigate integrierte **go2rtc-RTSP-Server, Port 8554** (`- "8554:8554"`). Ohne ihn können Sie weiterhin MJPEG oder Schnappschüsse verwenden (siehe [Livebild](#livebild)).
- Optional: ein von Frigate verwendeter **MQTT-Broker**, falls Sie MQTT dem Frigate-WebSocket vorziehen.
- Für H.265-Kameras (HEVC) benötigt Windows einen HEVC-Decoder. Die meisten PCs mit aktuellem Grafiktreiber haben einen; andernfalls installieren Sie die *HEVC-Videoerweiterungen* aus dem Microsoft Store oder verwenden einen H.264-Sub-Stream. Ist kein Decoder verfügbar, zeigt FrigatePiP automatisch stattdessen MJPEG von Frigate.
- **Windows-N-Editionen** (z. B. Windows 11 Pro N) enthalten Media Foundation erst, wenn Sie das optionale Windows-Feature *Media Feature Pack* hinzufügen. FrigatePiP funktioniert auch ohne, das Livebild kommt dann jedoch als MJPEG von Frigate (ca. 5 Bilder pro Sekunde), und AAC-Kameraton ist nicht verfügbar.

## 2. Installation

### Mit dem Installationsprogramm (empfohlen)

1. Laden Sie `FrigatePiP-<version>-installer.exe` von der Seite [Releases](https://github.com/maciejgornicz/frigate-pip/releases) herunter.
2. Führen Sie die Datei aus. Das Installationsprogramm benötigt keine Administratorrechte; es installiert nur für den aktuellen Benutzer, nach `%LOCALAPPDATA%\Programs\FrigatePiP`.
3. Wählen Sie, ob FrigatePiP **mit Windows starten** soll (standardmäßig aktiviert) und ob eine Desktopverknüpfung erstellt werden soll.
4. Lassen Sie am Ende **FrigatePiP starten** aktiviert. Das Tray-Symbol erscheint, und beim ersten Start öffnet sich das Einstellungsfenster.

Installationsprogramm und Anwendung sind nicht mit einem Code-Signing-Zertifikat signiert. Windows SmartScreen zeigt möglicherweise *„Der Computer wurde durch Windows geschützt“* an; klicken Sie auf **Weitere Informationen → Trotzdem ausführen**. Sie können die mit jedem Release veröffentlichte SHA-256-Prüfsumme kontrollieren.

### Portabel (ohne Installation)

Laden Sie `FrigatePiP-<version>-portable.exe` von der Releases-Seite herunter und starten Sie die Datei aus einem beliebigen Ordner. Die Einstellungen werden am selben Ort gespeichert wie bei der Installation (siehe [Dateien](#10-dateien-datenschutz-und-ressourcen)). Verwenden Sie in den Einstellungen **FrigatePiP mit Windows starten**, um es automatisch zu starten.

### Aktualisieren

FrigatePiP sucht einmal täglich nach einer neuen Version (**Nach Updates suchen** in den Einstellungen). Ist eine verfügbar, erscheint eine Benachrichtigung, und sowohl das Tray-Menü als auch das Einstellungsfenster bieten **Auf … aktualisieren** an: FrigatePiP lädt das Installationsprogramm von GitHub herunter, prüft seine SHA-256-Prüfsumme und führt es aus – das Update dauert wenige Sekunden, Ihre Einstellungen bleiben erhalten. **Jetzt prüfen** in den Einstellungen sucht sofort nach einer neuen Version. Sie können das Installationsprogramm einer neuen Version auch selbst ausführen. Die portable Version öffnet nur die Release-Seite: Laden Sie die neue EXE-Datei herunter und ersetzen Sie die alte.

### Deinstallieren

**Einstellungen → Apps → Installierte Apps → FrigatePiP → Deinstallieren** (oder *Programme hinzufügen oder entfernen*). Das Deinstallationsprogramm beendet FrigatePiP und entfernt das Programm, die Verknüpfungen und den Autostart-Eintrag. Ihre Einstellungen und das Protokoll in `%APPDATA%\FrigatePiP` bleiben erhalten; löschen Sie diesen Ordner, um auch sie zu entfernen.

## 3. Erster Start

1. Beim ersten Start öffnet sich das Einstellungsfenster auf der Registerkarte **Verbindung**.
2. Geben Sie die **Frigate-Adresse** ein, z. B. `http://192.168.1.10:5000`.
3. Klicken Sie auf **Verbindung testen**. FrigatePiP prüft, ohne etwas zu speichern:
   - die Frigate-API und ihre Version,
   - die Kameraliste,
   - die Ereignisquelle (WebSocket oder MQTT),
   - das RTSP-Video von go2rtc, einschließlich Codec und ob die Kamera Audio hat.

   Jede Zeile beginnt mit ✔ (funktioniert) oder ✖ (Problem, mit Hinweis).
4. Klicken Sie auf **OK**. Ab jetzt wartet FrigatePiP im Hintergrund auf Erkennungen.
5. Um zu sehen, wie ein Fenster aussieht, klicken Sie mit der rechten Maustaste auf das Tray-Symbol und wählen **Kamera anzeigen → (eine Kamera)**, oder aktivieren Sie **Live anzeigen** unten im Einstellungsfenster.

## 4. Das Tray-Symbol

Das Symbol im Infobereich der Taskleiste zeigt den Status:

| Farbe | Bedeutung |
|---|---|
| Blau | Mit Frigate verbunden, wartet auf Erkennungen |
| Grau | Nicht verbunden (FrigatePiP versucht es automatisch weiter) |
| Orange | Benachrichtigungen sind pausiert |

Zeigen Sie mit der Maus auf das Symbol, um den Status zu sehen (z. B. *Verbunden: http://192.168.1.10:5000*).

- **Linksklick** öffnet das Einstellungsfenster.
- **Rechtsklick** öffnet das Menü:

| Menüeintrag | Funktion |
|---|---|
| *(Statuszeile)* | Verbindungsstatus oder verbleibende Pausenzeit (nur zur Information). |
| **Auf … aktualisieren** | Erscheint, wenn eine neue Version verfügbar ist: lädt sie herunter und installiert sie (siehe [Aktualisieren](#aktualisieren)). |
| **Einstellungen…** | Öffnet das Einstellungsfenster. |
| **Kamera anzeigen** | Öffnet sofort ein Vorschaufenster der gewählten Kamera, auch während Benachrichtigungen pausiert sind. Nützlich, um das Bild zu prüfen oder einfach nachzusehen. |
| **Benachrichtigungen pausieren** | **Für 15 Minuten**, **Für 1 Stunde** oder **Bis zur Fortsetzung**. Während der Pause öffnen Erkennungen keine Fenster. Bei aktiver Pause lautet der Eintrag **Pausiert**, und **Jetzt fortsetzen** beendet sie. |
| **Vorschaufenster schließen** | Schließt alle geöffneten Vorschaufenster, auch angeheftete. |
| **Frigate öffnen** | Öffnet die Frigate-Weboberfläche in Ihrem Browser. |
| **Hilfe** | Öffnet dieses Handbuch. |
| **Protokoll anzeigen** | Öffnet die Protokolldatei im Editor (nützlich, wenn etwas nicht funktioniert). |
| **Einstellungsdatei (erweitert)…** | Öffnet `config.ini` im Editor. Gespeicherte Änderungen werden sofort wirksam. |
| **Beenden** | Beendet FrigatePiP. |

## 5. Vorschaufenster

Wenn eine Erkennung Ihren Einstellungen entspricht, erscheint in der gewählten Bildschirmecke ein Vorschaufenster:

1. Das Fenster erscheint sofort, wenige Millisekunden nach der Meldung von Frigate.
2. Es zeigt zuerst den neuesten Schnappschuss der Kamera aus Frigate und wechselt dann zum Livevideo, sobald der Stream ein Bild liefert (meist innerhalb einer Sekunde, mit dem [Vorpuffer](#vorpuffer) nahezu sofort).
3. Die Beschriftung (falls aktiviert) zeigt den Kameranamen, das Objekt (z. B. *Person*), das Sub-Label (z. B. ein erkanntes Gesicht oder Kennzeichen), die Erkennungssicherheit und die Uhrzeit. Ein **roter Punkt** bedeutet, dass das Bild live ist.

Vorschaufenster übernehmen nie den Tastaturfokus und unterbrechen daher weder das Tippen noch Spiele.

### Mausaktionen

| Aktion | Wirkung |
|---|---|
| **Ziehen** | Verschiebt das Fenster. Es bleibt dort, wo Sie es abgelegt haben, bis es sich schließt. |
| **Klicken** | Das Fenster wird **angeheftet**: Es bleibt geöffnet, bis Sie es schließen (die Beschriftung zeigt *angeheftet*). Erneut klicken, um es zu lösen. |
| **Mausrad** | Vergrößert oder verkleinert das Fenster. |
| **Strg + Mausrad** | Zoomt das Bild um den Mauszeiger herum heran (bis 8×) oder wieder heraus; die Beschriftung zeigt den Zoom, z. B. *2.0×*. Wie viele Details zu sehen sind, hängt vom Stream ab: der Main-Stream (`stream = main`) hat die meisten. |
| **Strg + Ziehen** | Bei gezoomtem Bild: verschiebt das Bild, um einen anderen Ausschnitt zu sehen. Ohne Zoom wird wie gewohnt das Fenster verschoben. |
| **×** (oben rechts) | Schließt das Fenster sofort. Die Schaltfläche erscheint, wenn Sie mit der Maus auf das Fenster zeigen. |
| **Lautsprecher** (links neben ×) | Schaltet den Ton des Fensters aus; ein weiterer Klick schaltet ihn wieder ein. Die Schaltfläche erscheint, wenn Sie auf ein Fenster mit Kameraton zeigen; solange der Ton aus ist, bleibt der durchgestrichene Lautsprecher sichtbar. |
| **Lautstärkeregler** (unter dem Lautsprecher) | Stellt die Lautstärke dieses Fensters ein: ziehen oder anklicken. Er erscheint zusammen mit dem Lautsprecher; Lauterstellen schaltet auch stummgeschalteten Ton wieder ein. Unter Linux ist die neue Lautstärke kurz nach dem Loslassen zu hören. |
| **Rechtsklick** | Schließt das Fenster. |
| **Doppelklick** | Öffnet diese Kamera in der Frigate-Weboberfläche. |
| **Darauf zeigen** | Solange sich der Mauszeiger über dem Fenster befindet, schließt es sich nicht. Nachdem Sie die Maus wegbewegt haben, bleibt es noch mindestens 3 Sekunden geöffnet. |

### Wann schließt sich ein Fenster?

- Nach der unter **Dauer nach letzter Bewegung** eingestellten Zahl von Sekunden, nachdem sich das Objekt (oder ein anderes passendes Objekt dieser Kamera) zuletzt bewegt hat.
- Spätestens nach der unter **höchstens** eingestellten Zahl von Sekunden, auch wenn sich das Objekt weiter bewegt.
- Nie, solange es angeheftet ist oder sich der Mauszeiger darüber befindet.

### Mehrere Kameras

Jede Kamera hat höchstens ein Fenster. Neue Erkennungen einer Kamera, die bereits ein Fenster hat, verlängern nur dessen Anzeige und aktualisieren die Beschriftung. Fenster verschiedener Kameras werden von der gewählten Ecke aus gestapelt. **Max. Anzahl Fenster** begrenzt, wie viele gleichzeitig geöffnet sind; **Wenn alle belegt sind** legt fest, ob eine neue Kamera das älteste Fenster ersetzt (an derselben Stelle auf dem Bildschirm; angeheftete Fenster werden nie ersetzt) oder übersprungen wird.

## 6. Das Einstellungsfenster

Öffnen Sie es per Linksklick auf das Tray-Symbol oder mit **Einstellungen…** im Menü. **Jede Option hat eine Beschreibung: Zeigen Sie mit der Maus darauf.** Drücken Sie **F1** oder klicken Sie auf **Hilfe**, um dieses Handbuch zu öffnen.

Die meisten Änderungen wirken sich **sofort** auf geöffnete Vorschaufenster aus, sodass Sie das Ergebnis vor dem Speichern sehen. **OK** speichert und schließt, **Übernehmen** speichert und lässt das Fenster geöffnet, **Abbrechen** (oder das Schließen des Fensters) verwirft nicht gespeicherte Änderungen.

### 6.1 Verbindung

| Option | Beschreibung |
|---|---|
| **Frigate-Adresse** | Adresse des Frigate-Servers mit Port: `http://192.168.1.10:5000` (Port 5000, ohne Anmeldung) oder `https://192.168.1.10:8971` (Port 8971, mit dem Frigate-Benutzer unten). Auch die Adresse eines Reverse-Proxys funktioniert. |
| **Frigate-Benutzer / Passwort** | Nur für Port 8971 oder einen Reverse-Proxy, der eine Frigate-Anmeldung verlangt: ein Frigate-Benutzer (in Frigate unter *Einstellungen → Benutzer*); die Rolle *viewer* genügt. Für Port 5000 leer lassen. FrigatePiP meldet sich an, behält das Token von Frigate und meldet sich erneut an, wenn es abläuft. Das Passwort wird wie das MQTT-Passwort in `config.ini` gespeichert. |
| **Ereignisquelle** | **Frigate-WebSocket (empfohlen)** — FrigatePiP verbindet sich direkt mit Frigate; sonst wird nichts benötigt. **MQTT** — Erkennungen werden von Ihrem MQTT-Broker gelesen (dem in Frigate konfigurierten). Beide liefern dasselbe Ergebnis; der WebSocket ist meist etwas schneller. |
| **MQTT-Server**, **Port** | Hostname oder IP-Adresse und Port (meist 1883) des MQTT-Brokers. Wird nur mit der Quelle MQTT verwendet. |
| **Benutzer / Passwort** | MQTT-Anmeldedaten. Leer lassen, wenn der Broker anonymen Zugriff erlaubt. |
| **MQTT-Topic-Präfix** | `mqtt.topic_prefix` aus der Frigate-Konfiguration, meist `frigate`. |
| **RTSP-Adresse (go2rtc)** | Vorlage der Adresse des Livevideos. `{host}` wird durch den Host der Frigate-Adresse und `{stream}` durch den Namen des go2rtc-Streams ersetzt. Der Standardwert `rtsp://{host}:8554/{stream}` passt zu einer Standardinstallation von Frigate. Wenn go2rtc eine Anmeldung erfordert, verwenden Sie `rtsp://user:password@{host}:8554/{stream}`. |
| **Verbindung testen** | Prüft die oben eingegebenen Werte (Frigate, Kameras, Ereignisse, RTSP-Video und Audio), ohne sie zu speichern. |
| **Sprache** | Sprache von FrigatePiP. **Automatisch (Windows)** folgt der Windows-Anzeigesprache; ist diese Sprache nicht verfügbar, wird Englisch verwendet. Siehe [Sprache](#9-sprache). |
| **FrigatePiP mit Windows starten** | Startet FrigatePiP automatisch, wenn Sie sich bei Windows anmelden. |
| **Nach Updates suchen** | Einmal täglich fragt FrigatePiP bei GitHub nach einer neuen Version und zeigt gegebenenfalls eine Benachrichtigung mit dem Update-Angebot. Es werden keine Daten über Sie oder Ihre Kameras gesendet. |
| **Jetzt prüfen** / **Auf … aktualisieren** | Sucht sofort nach einer neuen Version; wurde eine gefunden, lädt die Schaltfläche sie herunter und installiert sie (die Einstellungen bleiben erhalten). Der Text daneben zeigt das Ergebnis. |

### 6.2 Erkennung

| Option | Beschreibung |
|---|---|
| **Fenster anzeigen für** | Welche Frigate-Meldungen ein Fenster öffnen: **Jedes erkannte Objekt (am schnellsten)** — jedes von Frigate verfolgte Objekt; **Nur Review-Alarme** — nur Review-Einträge mit dem Schweregrad *alert*; **Review-Alarme und -Erkennungen** — alle Review-Einträge. Review-Einträge folgen den in der `review`-Konfiguration von Frigate festgelegten Regeln für Alarme und Erkennungen (Labels und erforderliche Zonen) und treffen etwas später ein als Objektereignisse. |
| **Kameras** | Kameras, die ein Fenster öffnen dürfen. Nicht ausgewählte Kameras werden ignoriert. Sind alle Kameras ausgewählt, werden auch später zu Frigate hinzugefügte Kameras berücksichtigt. |
| **Objekte** | Objekttypen (Labels), die ein Fenster öffnen dürfen, z. B. Person, Auto, Hund. Die Liste zeigt die von Ihren Kameras verfolgten Objekte. Sind alle ausgewählt, zählt jeder Objekttyp. |
| **Erforderliche Zonen** | Wenn Sie Zonen auswählen, öffnet sich ein Fenster nur, wenn das Objekt eine davon betritt. Ist keine ausgewählt, zählt jede Stelle im Bild. |
| **Unbewegte Objekte ignorieren** | Objekte, die Frigate als stationär markiert (z. B. ein geparktes Auto), öffnen keine Fenster und halten sie nicht offen. Gilt für *Jedes erkannte Objekt*. |
| **Min. Erkennungssicherheit** | Erkennungen mit geringerer Sicherheit werden ignoriert. 0 % bedeutet, dass die Schwellenwerte von Frigate entscheiden. Gilt für *Jedes erkannte Objekt*. |

Fehlalarme (False Positives) von Frigate werden immer ignoriert. Jedes Objekt öffnet nur einmal ein Fenster; bewegt es sich erneut, verlängert es nur das geöffnete Fenster.

Auch **manuelle Ereignisse** öffnen ein Fenster: über Frigates API erstellte Ereignisse (`POST /api/events/<kamera>/<label>/create`, z. B. von einer Home-Assistant-Automatisierung, wenn es klingelt) und Einträge einer eigenen LPR-Kamera. Die Beschriftung zeigt das Label des Ereignisses. Das funktioniert mit **Jedes erkannte Objekt (am schnellsten)** und **Review-Alarme und -Erkennungen**, mit **Nur Review-Alarme** dann, wenn Frigate das Ereignis zum Alarm macht (sein Label gehört zu den Alarm-Labels von Review, standardmäßig `person` und `car`). Zum Testen in PowerShell: `Invoke-RestMethod -Method Post -ContentType application/json -Body '{"duration": 10}' http://<frigate>:5000/api/events/<camera>/doorbell/create`; unter Linux: `curl -X POST -H "Content-Type: application/json" -d '{"duration": 10}' http://<frigate>:5000/api/events/<camera>/doorbell/create`. Die Filter für Kameras, Objekte und Zonen gelten wie für andere Erkennungen (ein Objektfilter `person, car` blendet ein `doorbell`-Ereignis aus); die Mindestsicherheit gilt nicht. Frigate legt den Review-Eintrag, auf den FrigatePiP reagiert, nur an, wenn die Kamera aufzeichnet (`record` aktiviert). Ein manuelles Ereignis, das entsteht, während der Review-Eintrag der Kamera noch offen ist (bis zur Alarm-Abschaltzeit, standardmäßig 40 s nach dem vorigen Ereignis), wird diesem Eintrag hinzugefügt, und Frigate meldet es erst, wenn sich ein Objekt auf der Kamera bewegt — warten Sie beim Testen daher etwa eine Minute zwischen zwei Ereignissen.

### 6.3 Vorschaufenster

| Option | Beschreibung |
|---|---|
| **Monitor** | Monitor, auf dem die Fenster erscheinen: **Hauptmonitor** oder ein bestimmter Monitor (von links nummeriert, mit seiner Auflösung). |
| **Bildschirmecke** | Ecke, in der die Fenster erscheinen. Weitere Fenster werden von dieser Ecke aus gestapelt. |
| **Fensterbreite** | Breite in Pixeln (bei 100 % Skalierung; FrigatePiP passt sie an die Skalierung des Monitors an). Die Höhe ergibt sich aus dem Seitenverhältnis der Kamera. Die Größe eines einzelnen Fensters können Sie auch mit dem Mausrad ändern. |
| **Abstand zum Rand** | Abstand zwischen den Fenstern und den Bildschirmrändern (und zwischen den Fenstern). |
| **Dauer nach letzter Bewegung** | Wie viele Sekunden ein Fenster nach der letzten Bewegung geöffnet bleibt. |
| **höchstens** | Maximale Anzeigedauer eines Fensters in Sekunden, auch wenn sich das Objekt weiter bewegt (es sei denn, es ist angeheftet). |
| **Max. Anzahl Fenster** | Wie viele Fenster (Kameras) gleichzeitig geöffnet sein können (1–8). |
| **Wenn alle belegt sind** | **Ältestes Fenster ersetzen (gleiche Position)** — die neue Kamera nimmt den Platz des ältesten nicht angehefteten Fensters ein; **Neues Ereignis überspringen** — die neue Erkennung wird ignoriert. |
| **Immer im Vordergrund** | Hält die Fenster über allen anderen Fenstern. |
| **Beschriftung im Bild** | Zeigt Kamera, Objekt, Erkennungssicherheit und Uhrzeit im Bild an. |
| **Benachrichtigungston** | Spielt den Windows-Sound *Sternchen* ab, wenn ein neues Fenster erscheint. Dies ist nicht der Kameraton (siehe [Kameraton](#kameraton)). |
| **Während Spielen oder Präsentationen im Vollbild nicht anzeigen** | Es erscheinen keine Fenster, während ein Spiel, ein Video oder eine Präsentation im Vollbild läuft (Erkennungen werden übersprungen, nicht verzögert). |

### 6.4 Video & Audio

#### Livebild

| Auswahl | Beschreibung |
|---|---|
| **RTSP von go2rtc** (Standard) | Flüssiges Video mit der Bildrate der Kamera und Hardware-Decodierung (H.264 und H.265), geringe Verzögerung. Benötigt Port 8554. |
| **MJPEG von Frigate** | Ca. 5 Bilder pro Sekunde in der Erkennungsauflösung (detect), über die Frigate-API. Verwenden Sie diese Option, wenn RTSP nicht verfügbar ist. |
| **Nur aktualisierte Schnappschüsse** | Nur die Schnappschüsse von Frigate, regelmäßig aktualisiert. Die sparsamste Option. |

#### Stream

| Auswahl | Beschreibung |
|---|---|
| **Automatisch (nach Fenstergröße)** | Der ressourcenschonende **sub**-Stream für normale Fenster, der **main**-Stream in voller Auflösung, wenn das Fenster breiter ist als **Main ab Fensterbreite** (in physischen Pixeln). |
| **Sub (ressourcenschonend)** | Immer der Sub-Stream. |
| **Main (volle Auflösung)** | Immer der Main-Stream. |

FrigatePiP ermittelt die go2rtc-Streams jeder Kamera aus der Frigate-Konfiguration (`live.streams`, `live.stream_name` oder go2rtc-Streams namens `<camera>` und `<camera>_sub`). Sie können sie in der [Einstellungsdatei](#einstellungen-pro-kamera) auch selbst festlegen.

#### Kameraton

**Kameraton** gibt den Ton der Kamera (AAC oder G.711 aus dem RTSP-Stream) in jedem geöffneten Fenster wieder. Er läuft separat und verzögert daher nie das Fenster oder das Video. Die Lautstärke lässt sich im Lautstärkemixer von Windows einstellen (*FrigatePiP*). Erfordert **RTSP von go2rtc**. Um ein einzelnes Fenster stummzuschalten, klicken Sie auf die Lautsprecher-Schaltfläche oben rechts im Fenster (siehe [Mausaktionen](#mausaktionen)). **Lautstärke** legt fest, wie laut ein neues Fenster startet (0–100 %, `camera_volume`), zum Beispiel gegen Windgeräusche; jedes Fenster hat außerdem einen eigenen Regler.

#### Vorpuffer

**Vorpuffer** hält eine Verbindung zum Sub-Stream jeder Kamera, sodass das Livevideo sofort startet (ca. 0,15 s), statt auf das nächste Schlüsselbild zu warten (bis zu ca. 1 s, je nach Schlüsselbildintervall der Kamera). Kosten: ständig ca. 0,1–0,5 Mbit/s Datenverkehr im lokalen Netzwerk pro Kamera.

#### GPU bereithalten

**GPU bereithalten** hält das Grafikgerät initialisiert, sodass das Video ca. 130 ms früher startet. Kosten: ca. 1 % eines CPU-Kerns im Leerlauf (Thread des Grafiktreibers).

### 6.5 Unterer Fensterbereich

| Element | Beschreibung |
|---|---|
| **Vorschau:** Kamera | Kamera für die Live-Vorschau der Einstellungen. |
| **Live anzeigen** | Öffnet ein echtes Vorschaufenster dieser Kamera. Es übernimmt Ihre Änderungen (Ecke, Größe, Beschriftung, Stream, Audio …) sofort, noch bevor Sie sie speichern. Es ist angeheftet und zählt nicht zum Fensterlimit. |
| **OK** / **Abbrechen** / **Übernehmen** | Speichern und schließen / verwerfen / speichern und Fenster geöffnet lassen. |
| **Hilfe** | Öffnet dieses Handbuch (auch **F1**). |

## 7. Die Einstellungsdatei

Alle Einstellungen werden in `%APPDATA%\FrigatePiP\config.ini` gespeichert (eine reine Textdatei). Das Einstellungsfenster schreibt sie alle; die Datei benötigen Sie nur für Einstellungen pro Kamera. **Einstellungsdatei (erweitert)…** im Tray-Menü öffnet sie im Editor. Änderungen werden beim Speichern der Datei automatisch übernommen. Kommentare beginnen mit `;`. Ja/Nein-Werte akzeptieren `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`.

| Abschnitt | Schlüssel | Standard | Bedeutung |
|---|---|---|---|
| `[app]` | `language` | `auto` | `auto` (Windows-Sprache) oder `en`, `pl`, `de`, `es`, `pt`, `fr`. |
| | `check_updates` | `true` | Einmal täglich nach einer neuen Version suchen. |
| `[frigate]` | `url` | `http://frigate.local:5000` | Frigate-Adresse. |
| | `user`, `password` | *(leer)* | Frigate-Anmeldung für Port 8971 (leer für Port 5000). |
| | `source` | `websocket` | `websocket` oder `mqtt`. |
| | `rtsp` | `rtsp://{host}:8554/{stream}` | Vorlage der RTSP-Adresse. |
| `[mqtt]` | `host` | *(leer)* | MQTT-Broker (nur mit `source = mqtt`). |
| | `port` | `1883` | Port des Brokers. |
| | `user`, `password` | *(leer)* | Anmeldedaten. |
| | `prefix` | `frigate` | Topic-Präfix. |
| `[trigger]` | `mode` | `events` | `events` (jedes Objekt), `alerts` (Review-Alarme) oder `reviews` (Alarme und Erkennungen). |
| | `cameras` | *(leer = alle)* | Kommagetrennte Kameranamen, z. B. `front_door, garden`. |
| | `labels` | *(leer = alle)* | Kommagetrennte Objekte, z. B. `person, car`. |
| | `zones` | *(leer = überall)* | Erforderliche Zonen. |
| | `ignore_stationary` | `true` | Unbewegte Objekte ignorieren. |
| | `min_score` | `0` | Mindest-Erkennungssicherheit 0.0–1.0. |
| `[display]` | `monitor` | `primary` | `primary` oder Monitornummer von links (1, 2, 3 …). |
| | `position` | `bottom-right` | `top-left`, `top-right`, `bottom-left`, `bottom-right`. |
| | `margin` | `16` | Abstand zum Bildschirmrand in Pixeln. |
| | `width` | `480` | Fensterbreite in Pixeln. |
| | `duration` | `15` | Sekunden nach der letzten Bewegung. |
| | `max_duration` | `120` | Maximale Anzeigedauer eines Fensters in Sekunden. |
| | `topmost` | `true` | Immer im Vordergrund. |
| | `sound` | `false` | Benachrichtigungston. |
| | `overlay` | `true` | Beschriftung im Bild. |
| | `max_popups` | `4` | Max. Anzahl Fenster. |
| | `replace` | `true` | `true` = ältestes Fenster ersetzen, `false` = neues Ereignis überspringen. |
| | `skip_fullscreen` | `false` | Bei Vollbildanwendungen nicht anzeigen. |
| `[video]` | `live` | `rtsp` | `rtsp`, `mjpeg` oder `snapshot`. |
| | `stream` | `auto` | `auto`, `sub` oder `main`. |
| | `auto_main_width` | `800` | Fensterbreite (px), ab der `auto` den Main-Stream verwendet. |
| | `prebuffer` | `false` | Vorpuffer. |
| | `gpu_warm` | `false` | GPU bereithalten. |
| | `camera_audio` | `false` | Kameraton. |
| | `camera_volume` | `100` | Lautstärke eines neuen Fensters, 0–100 %. |

### Einstellungen pro Kamera

Fügen Sie einen Abschnitt `[camera.<name>]` hinzu, wobei `<name>` der Kameraname in Frigate ist:

```ini
[camera.front_door]
; Name shown in the caption, the menu and the settings
name = Front door
; go2rtc stream names
main = front_door
sub = front_door_sub
```

## 8. Befehlszeile

| Befehl | Wirkung |
|---|---|
| `frigate-pip.exe` | Startet FrigatePiP (es läuft immer nur eine Instanz). |
| `frigate-pip.exe --settings` | Öffnet das Einstellungsfenster (startet FrigatePiP, falls es nicht läuft). |
| `frigate-pip.exe --test <camera>` | Zeigt sofort ein Vorschaufenster der Kamera, z. B. `--test front_door`. Sie können eine Windows-Verknüpfung mit Tastenkombination für Ihre Lieblingskamera anlegen. |
| `frigate-pip.exe --quit` | Beendet das laufende FrigatePiP. |

Läuft FrigatePiP bereits, wird der Befehl an die laufende Instanz übergeben.

## 9. Sprache

FrigatePiP ist auf Englisch, Polnisch, Deutsch, Spanisch, Portugiesisch und Französisch verfügbar. Mit **Automatisch (Windows)** verwendet es die Windows-Anzeigesprache (*Einstellungen → Zeit und Sprache → Sprache und Region → Windows-Anzeigesprache*) oder Englisch, falls diese Sprache nicht verfügbar ist. Sie können die Sprache unter **Verbindung → Sprache** auch selbst wählen; das Einstellungsfenster wechselt sofort.

## 10. Dateien, Datenschutz und Ressourcen

| Was | Wo |
|---|---|
| Programm | `%LOCALAPPDATA%\Programs\FrigatePiP\frigate-pip.exe` (Installationsprogramm) |
| Einstellungen | `%APPDATA%\FrigatePiP\config.ini` |
| Protokoll | `%APPDATA%\FrigatePiP\frigate-pip.log` (bis zu 512 KB; das vorherige wird als `frigate-pip.old.log` aufbewahrt) |
| Autostart | Registrierungswert `FrigatePiP` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |

**Datenschutz:** FrigatePiP verbindet sich nur mit den Adressen, die Sie eingeben (Frigate, go2rtc und der MQTT-Broker), und – sofern Sie **Nach Updates suchen** nicht ausschalten – einmal täglich mit GitHub, um nach einer neuen Version zu suchen. Es gibt keine Telemetrie, keine Konten und keine Clouddienste. Bilder werden nur angezeigt und nie gespeichert. Die Links unter **Hilfe** öffnen dieses Handbuch auf GitHub in Ihrem Browser.

**Ressourcen:** Während FrigatePiP auf Erkennungen wartet, verbraucht es praktisch keine CPU-Leistung und ca. 5–10 MB Arbeitsspeicher. Video wird nach Möglichkeit von der Grafikkarte decodiert. Ohne nutzbare Grafikkarte (z. B. in manchen virtuellen Maschinen) zeichnet es mit dem Software-Renderer WARP.

## 11. Fehlerbehebung

Beginnen Sie mit **Verbindung testen** in den Einstellungen und mit **Protokoll anzeigen** im Tray-Menü.

**Das Symbol bleibt grau (nicht verbunden)**
- Prüfen Sie die Frigate-Adresse und den Port in einem Browser auf demselben PC: `http://<frigate>:5000/api/version` (oder nach der Anmeldung `https://<frigate>:8971/api/version`) sollte die Version anzeigen.
- Unter Docker muss der Port (5000 oder 8971) veröffentlicht sein.
- Home-Assistant-Add-on: Aktivieren Sie Port 5000 in den *Netzwerk*-Einstellungen des Add-ons oder verwenden Sie Port 8971 mit einem Frigate-Benutzer.
- Port 8971 oder ein Reverse-Proxy: Geben Sie Frigate-Benutzer und Passwort ein; **Verbindung testen** zeigt, ob die Anmeldung funktioniert. Einem selbstsignierten Zertifikat (dem eigenen Zertifikat von Frigate auf Port 8971) wird beim ersten Mal vertraut, und es wird in `certificates.txt` (neben `config.ini`) gespeichert; zeigt der Server später ein anderes, verweigert FrigatePiP die Verbindung – wenn Sie das Zertifikat selbst ersetzt haben, löschen Sie seine Zeile in dieser Datei.
- Bei MQTT: Prüfen Sie Broker-Adresse, Anmeldedaten und Topic-Präfix.

**Es erscheinen keine Fenster**
- Ist das Symbol orange? Dann sind die Benachrichtigungen pausiert — wählen Sie **Jetzt fortsetzen**.
- Prüfen Sie die Registerkarte **Erkennung**: Kameras, Objekte und erforderliche Zonen. Sind erforderliche Zonen ausgewählt, müssen Objekte sie betreten.
- Die *Review*-Modi hängen von der Review-Konfiguration in Frigate ab; versuchen Sie **Jedes erkannte Objekt**.
- **Während Spielen oder Präsentationen im Vollbild nicht anzeigen** überspringt Fenster, solange eine Vollbildanwendung im Vordergrund ist.
- **Max. Anzahl Fenster** zusammen mit **Neues Ereignis überspringen** überspringt Erkennungen, wenn alle Fenster geöffnet sind.
- **Min. Erkennungssicherheit** ist möglicherweise zu hoch eingestellt.

**Das Fenster zeigt nur ein Standbild**
- Livevideo benötigt den RTSP-Port 8554 von go2rtc. Prüfen Sie die Zeile **RTSP-Video** von **Verbindung testen**.
- Wenn go2rtc eine Anmeldung erfordert, tragen Sie die Anmeldedaten in die **RTSP-Adresse** ein.
- Installieren Sie für H.265-Kameras die *HEVC-Videoerweiterungen* oder verwenden Sie einen H.264-Sub-Stream.
- Wählen Sie als Notlösung **MJPEG von Frigate**.
- Unter Windows N ohne *Media Feature Pack* ist RTSP-Video nicht verfügbar; FrigatePiP verwendet automatisch MJPEG (im Protokoll: *MJPEG from Frigate instead of RTSP*).

**Das Video startet langsam**
- Aktivieren Sie **Vorpuffer** und **GPU bereithalten**.
- Verkürzen Sie das Schlüsselbildintervall (I-Frame-Intervall / GOP) des Sub-Streams der Kamera auf 1–2 s.

**Kein Kameraton**
- **Kameraton** muss aktiviert und **Livebild** auf **RTSP von go2rtc** eingestellt sein.
- Der go2rtc-Stream muss Audio enthalten — **Verbindung testen** zeigt *Audio … Hz* oder *kein Audio* an.
- Prüfen Sie die Lautstärke von *FrigatePiP* im Lautstärkemixer von Windows.

**Etwas anderes** — Erstellen Sie ein [Issue auf GitHub](https://github.com/maciejgornicz/frigate-pip/issues) und hängen Sie den relevanten Teil des Protokolls an. Entfernen Sie Adressen und Kameranamen, die Sie nicht weitergeben möchten.

---

## 12. Linux-Version

FrigatePiP läuft auch unter Linux — auf einem **Desktop** (GNOME, KDE, Xfce…) wie unter Windows und **ohne Desktop**, zum Beispiel auf einem Raspberry Pi mit einem Monitor an der Wand oder auf einem Server, den Sie per SSH erreichen. Es zeigt dieselben Vorschaufenster, in einem von vier Modi:

| Modus | Wo die Fenster erscheinen | Läuft im Hintergrund? | Typische Verwendung | Start |
|---|---|---|---|---|
| **Desktop-Fenster** | Auf dem Desktop (X11 oder Wayland mit XWayland): rahmenlose Fenster über allen anderen in der gewählten Ecke, wie unter Windows | **Ja** — ohne Terminal; aus dem Anwendungsmenü oder mit der Desktop-Sitzung | Ein Linux-PC oder -Laptop | Menü → **FrigatePiP** oder `frigate-pip` auf dem Desktop — siehe [Desktop-Fenster](#desktop-fenster) |
| **Framebuffer** | Auf dem Bildschirm einer Linux-Konsole ohne X oder Wayland, in der gewählten Ecke, über dem Konsolentext | **Ja** — als systemd-Dienst | Ein Raspberry Pi oder ein anderer Computer mit Monitor und ohne Desktop (ein Wanddisplay) | `frigate-pip --output framebuffer`, als Dienst: siehe [Automatischer Start](#automatischer-start-systemd) |
| **Terminal** | Im Terminalfenster, in dem FrigatePiP läuft: eine Beschriftungszeile, das Bild und eine Statuszeile | **Nein** — das Terminal zeigt FrigatePiP, solange es läuft (wie `top`) | Ein Terminalfenster oder tmux-Bereich, der als Kameramonitor geöffnet bleibt, auch per SSH | `frigate-pip` |
| **tmux-Popups** | Als tmux-Popup über dem, woran Sie gerade in tmux arbeiten; es schließt sich von selbst | **Ja** — in einem verborgenen tmux-Fenster | Arbeit in tmux (lokal oder per SSH), wenn Kameras automatisch eingeblendet werden sollen | `tmux new -d -s frigate-pip 'frigate-pip --output tmux'`, danach in tmux arbeiten — siehe [tmux-Popups](#tmux-popups) |

**Auf einem Windows-PC — auch wenn Sie in WSL arbeiten — verwenden Sie die Windows-Anwendung:** Sie läuft im Infobereich, und ihre Fenster erscheinen über allem, auch über Terminals. Die Linux-Version in WSL ist nur zum Ausprobieren sinnvoll.

Wenn sich auf dem Framebuffer ein Fenster schließt, erscheint wieder, was darunter war. Im Terminal und in tmux-Popups wird das Bild mit **Kitty-Grafik** (Kitty, WezTerm, Ghostty), **Sixel** (foot, Konsole, xterm, Windows Terminal 1.22+) oder, in jedem Terminal mit 24-Bit-Farbe, mit farbigen Halbblöcken gezeichnet (geringe Auflösung; tmux-Popups verwenden immer Halbblöcke).

Das Video wird vom Programm **ffmpeg** decodiert; der eigene RTSP-Client von FrigatePiP versorgt es mit Daten, sodass ein Bild wenige Millisekunden nach dem nächsten Schlüsselbild der Kamera erscheint.

### Voraussetzungen

- Linux auf **x86_64** oder **ARM64** (Raspberry Pi 3, 4 oder 5 mit 64-Bit-System).
- Für Desktop-Fenster: ein Desktop mit X11 oder Wayland mit XWayland (GNOME, KDE Plasma, Xfce und die meisten anderen).
- **ffmpeg**: wird mit dem `.deb`-Paket mitinstalliert; sonst `sudo apt install ffmpeg` (Debian, Ubuntu, Raspberry Pi OS) oder das entsprechende Paket Ihrer Distribution.
- Für Beschriftungen im Framebuffer: die Schriftart DejaVu (`fonts-dejavu-core`, wird normalerweise zusammen mit ffmpeg installiert).
- Für den Framebuffer: Mitgliedschaft in der Gruppe `video` (`sudo usermod -aG video $USER`, danach neu anmelden).
- Frigate muss von diesem Rechner aus über seinen API-Port 5000 (`http://`) oder 8971 (`https://`, mit einem Frigate-Benutzer) und, für Livevideo, über den RTSP-Port 8554 von go2rtc erreichbar sein (Einzelheiten unter [Voraussetzungen](#1-voraussetzungen)).

### Installation

1. Installieren Sie das Programm:
   - **Debian, Ubuntu, Raspberry Pi OS (64-Bit), aus dem FrigatePiP-Repository** (empfohlen — dann aktualisiert `sudo apt upgrade` auch FrigatePiP): Fügen Sie das Repository einmal hinzu und installieren Sie das Programm; apt installiert ffmpeg gleich mit:

     ```
     sudo wget -qO /usr/share/keyrings/frigate-pip.gpg https://maciejgornicz.github.io/frigate-pip/apt/frigate-pip.gpg
     echo "deb [signed-by=/usr/share/keyrings/frigate-pip.gpg] https://maciejgornicz.github.io/frigate-pip/apt stable main" | sudo tee /etc/apt/sources.list.d/frigate-pip.list
     sudo apt update
     sudo apt install frigate-pip
     ```

   - **Debian, Ubuntu, Raspberry Pi OS (64-Bit), einzelne Paketdatei** — laden Sie `frigate-pip_<version>_amd64.deb` (oder `frigate-pip_<version>_arm64.deb` für einen Raspberry Pi) von der Seite [Releases](https://github.com/maciejgornicz/frigate-pip/releases) herunter und installieren Sie es mit apt, das ffmpeg gleich mitinstalliert:

     ```
     sudo apt install ./frigate-pip_*.deb
     ```

   - **Andere Distributionen** — laden Sie `FrigatePiP-<version>-linux-x86_64.tar.gz` oder `FrigatePiP-<version>-linux-arm64.tar.gz` herunter, entpacken Sie das Archiv und legen Sie das Programm in ein Verzeichnis im Suchpfad:

     ```
     tar xzf FrigatePiP-*-linux-*.tar.gz
     sudo install frigate-pip /usr/local/bin/
     ```

2. Starten Sie es einmal: Es legt `~/.config/frigate-pip/config.ini` an. Setzen Sie `url` im Abschnitt `[frigate]` auf Ihre Frigate-Adresse.
3. Prüfen Sie die Verbindung: `frigate-pip --test-connection`.
4. Starten Sie `frigate-pip`. Mit `--test <camera>` (oder den Tasten **1–9**) sehen Sie sofort eine Kamera.

Das Programm ist eine einzelne Datei ohne weitere Bibliotheken. Zum Aktualisieren führen Sie `sudo apt update && sudo apt upgrade` aus (Repository), installieren die neuere Paketdatei auf dieselbe Weise oder ersetzen die Datei; zum Entfernen führen Sie `sudo apt remove frigate-pip` aus (oder löschen die Datei) und löschen `~/.config/frigate-pip` — und, falls Sie das Repository hinzugefügt haben, `/etc/apt/sources.list.d/frigate-pip.list` und `/usr/share/keyrings/frigate-pip.gpg`.

### Wo die Fenster erscheinen

Mit `--output auto` (Standard) öffnet FrigatePiP **Desktop-Fenster**, wenn es auf einem Desktop läuft (`DISPLAY` ist gesetzt), verwendet den **Framebuffer** auf einer Linux-Konsole (`TERM=linux`) oder ohne Terminal (als Dienst) und andernfalls das **Terminal**. Der Modus **tmux-Popups** wird nur verwendet, wenn Sie ihn auswählen (`--output tmux` oder `output = tmux`). Im Terminal wird der Bildmodus automatisch gewählt: Kitty-Grafik, wo verfügbar, sonst Sixel, wenn das Terminal es meldet, sonst Halbblöcke. Beides lässt sich in der Einstellungsdatei oder in der Befehlszeile festlegen.

### Desktop-Fenster

Auf einem Linux-Desktop funktionieren die Vorschaufenster wie unter Windows: rahmenlose Fenster über allen anderen, in der gewählten Ecke (`position`) des gewählten Monitors (`monitor`: `primary` oder eine von links gezählte Nummer), so breit wie `width` (multipliziert mit der Desktop-Skalierung). Sie brauchen X11 oder Wayland mit XWayland — GNOME, KDE Plasma und die meisten anderen Desktops starten XWayland für solche Programme. Reines Wayland genügt nicht: Es erlaubt einem Programm nicht, ein Fenster in eine Ecke zu setzen oder im Vordergrund zu halten.

- **Start:** Anwendungsmenü → **FrigatePiP** (mit apt installiert) oder `frigate-pip` in einem Terminal auf dem Desktop. Es läuft im Hintergrund ohne Terminal; pro Benutzer läuft eine Kopie. Ein zweiter Start (oder `frigate-pip --settings`) öffnet die Einstellungen der laufenden Kopie.
- **Mit der Desktop-Sitzung:** `frigate-pip --autostart on` (wieder aus: `frigate-pip --autostart off`). Zum Beenden: `frigate-pip --quit`.
- **Symbol im Infobereich:** mit demselben Menü wie unter Windows (siehe [Das Tray-Symbol](#4-das-tray-symbol)); seine Farbe zeigt den Zustand: blau — verbunden, grau — nicht verbunden, orange — pausiert. Es erscheint unter KDE Plasma, Xfce, Cinnamon, MATE und Budgie sowie unter GNOME mit der Erweiterung AppIndicator (in Ubuntu enthalten).
- **Maus:** wie unter Windows (siehe [Mausaktionen](#mausaktionen)) — × oder die rechte Maustaste schließt, der Lautsprecher schaltet stumm, der Regler darunter stellt die Lautstärke ein, Ziehen verschiebt das Fenster, das Mausrad ändert seine Größe, Strg + Mausrad zoomt das Bild um den Mauszeiger, Strg + Ziehen verschiebt das gezoomte Bild, ein Klick heftet es an, ein Doppelklick öffnet die Kamera in Frigate, und ein Fenster unter der Maus schließt sich nicht.
- **Fensteroptionen:** `topmost` (bei `false` sind die Fenster gewöhnliche Fenster, die andere verdecken können), `sound` (der Benachrichtigungston des Desktops) und `skip_fullscreen` (nichts, solange das aktive Fenster im Vollbild ist) wirken wie unter Windows.
- **Einstellungen:** Ein Klick auf das Symbol (oder **Einstellungen…** in seinem Menü) öffnet sie im Browser — mit denselben Optionen, Beschreibungen und demselben Verbindungstest wie unter Windows; die Seite lässt sich nur auf diesem Rechner öffnen. Beim ersten Start öffnet sie sich von selbst. Gespeichert wird in `~/.config/frigate-pip/config.ini` (unten beschrieben).

### tmux-Popups

In diesem Modus hat FrigatePiP kein eigenes Fenster: Es wartet im Hintergrund auf Erkennungen und öffnet für jede ein [tmux](https://github.com/tmux/tmux)-Popup mit der Kamera über den Bereichen des zuletzt verwendeten tmux-Clients. Erforderlich sind tmux 3.2 oder neuer und FrigatePiP 1.3.0 oder neuer (ältere Versionen kennen `--output tmux` nicht und verwenden stillschweigend den Terminalmodus).

**Die Popups erscheinen nur, während Sie in tmux arbeiten.** tmux kann sie nur auf seinem eigenen Bildschirm zeichnen: In einem Terminalfenster ohne tmux — einer normalen Shell oder einer SSH-Sitzung außerhalb von tmux — wird nichts angezeigt, und Erkennungen werden nur ins Protokoll geschrieben. Den Hintergrundprozess zu starten genügt daher nicht; Sie brauchen auch eine tmux-Sitzung, in der Sie arbeiten:

1. Starten Sie FrigatePiP einmal in einer verborgenen tmux-Sitzung: `tmux new -d -s frigate-pip 'frigate-pip --output tmux'`.
2. Öffnen Sie tmux für Ihre eigene Arbeit — `tmux` startet eine neue Sitzung — und arbeiten Sie dort. Die Popups erscheinen über diesem Fenster.

- Zum Beenden von FrigatePiP: `tmux kill-session -t frigate-pip`. Um seine Tasten zu verwenden (Pause, Kamera anzeigen): `tmux attach -t frigate-pip`.
- Das Popup erscheint in der gewählten **Bildschirmecke** (`position`), und seine Größe richtet sich nach der **Fensterbreite** (`width`). Es schließt sich nach der üblichen Zeit von selbst (`duration`, `max_duration`) oder sofort mit **q**, **x** oder **Esc**. **m** schaltet seinen Ton aus und wieder ein.
- tmux zeigt immer nur ein Popup gleichzeitig: Eine Erkennung auf einer anderen Kamera ersetzt das geöffnete Popup (oder wird bei `replace = false` übersprungen).
- Für die vollen Farben des Bildes muss tmux 24-Bit-Farbe weitergeben: Fügen Sie `set -as terminal-features ",xterm-256color:RGB"` zu `~/.tmux.conf` hinzu (verwenden Sie dabei Ihren `$TERM` außerhalb von tmux) und starten Sie tmux neu. FrigatePiP schreibt diesen Hinweis in sein Protokoll, wenn die Farben fehlen.

### Tasten

Wenn FrigatePiP in einem Terminal oder auf einer Konsole läuft:

| Taste | Wirkung |
|---|---|
| **q** (oder Strg+C) | Beenden. |
| **x** (oder Esc) | Alle Vorschaufenster schließen. |
| **p** | Benachrichtigungen pausieren / fortsetzen. |
| **m** | Ton aus / ein (bei `camera_audio = true`) für alle Fenster, auch später geöffnete; die Beschriftung zeigt dann *Ton aus*. |
| **1**–**9** | Kamera 1–9 anzeigen (in der Reihenfolge der Frigate-Konfiguration). |

### Befehlszeile

| Option | Wirkung |
|---|---|
| `--output auto\|window\|framebuffer\|terminal\|tmux` | Wo die Fenster gezeichnet werden (siehe die Tabelle der Modi oben). |
| `--graphics auto\|kitty\|sixel\|blocks` | Wie Bilder im Terminal gezeichnet werden. |
| `--framebuffer /dev/fb0` | Framebuffer-Gerät. |
| `--hwaccel auto\|on\|off` | Videodekodierung auf der Grafikkarte (siehe `hwaccel` unten). |
| `--test <camera>` | Eine Kamera direkt nach dem Start anzeigen. |
| `--test-connection` | Die Verbindung zu Frigate prüfen (wie **Verbindung testen** unter Windows) und beenden. |
| `--list` | Listet die Kameras mit ihren Objekten und Zonen auf — die Namen für `cameras`, `labels` und `zones` in `[trigger]` — und beendet sich. |
| `--autostart on\|off` | Startet die Desktop-Fenster mit der Desktop-Sitzung (ein Eintrag in `~/.config/autostart`) oder nicht mehr. |
| `--quit` | Beendet die laufenden Desktop-Fenster. |
| `--settings` | Öffnet die Einstellungen der laufenden Desktop-Fenster (oder startet sie mit geöffneten Einstellungen). |
| `--version`, `--help` | Version, Kurzhilfe. |

### Einstellungen

Die Linux-Version verwendet dieselbe `config.ini` (in `~/.config/frigate-pip/` oder `$XDG_CONFIG_HOME/frigate-pip/`) wie die Windows-Version — siehe [Die Einstellungsdatei](#7-die-einstellungsdatei). Änderungen werden innerhalb von zwei Sekunden nach dem Speichern der Datei übernommen. Folgende Einstellungen werden verwendet: alles in `[app]` (Sprache), `[frigate]`, `[mqtt]`, `[trigger]`, in `[display]`: `monitor`, `position`, `margin`, `width`, `duration`, `max_duration`, `overlay`, `max_popups`, `replace`, `topmost`, `sound`, `skip_fullscreen`; in `[video]`: `live`, `stream`, `auto_main_width`, `prebuffer` (nicht für tmux-Popups), `camera_audio`; sowie die Abschnitte `[camera.<name>]`. Die nur unter Windows verfügbaren Einstellungen (`gpu_warm`, `check_updates`) werden ignoriert. `frigate-pip --list` zeigt die Namen Ihrer Kameras, Objekte und Zonen für die Filter in `[trigger]`.

Zusätzliche Einstellungen im Abschnitt `[console]`:

| Schlüssel | Standard | Bedeutung |
|---|---|---|
| `output` | `auto` | `auto`, `window`, `framebuffer`, `terminal` oder `tmux`. |
| `graphics` | `auto` | `auto`, `kitty`, `sixel` oder `blocks` (Bilder im Terminal). |
| `framebuffer` | `/dev/fb0` | Framebuffer-Gerät. |
| `ffmpeg` | `ffmpeg` | ffmpeg-Programm (Name oder vollständiger Pfad). |
| `hwaccel` | `auto` | Videodekodierung auf der Grafikkarte (VAAPI, VDPAU, NVIDIA… was ffmpeg findet; ohne eine auf dem Prozessor): `auto` = für den Main-Stream, wo es am meisten spart (das erste Bild eines solchen Fensters kann etwa 0,2 s später kommen; der Vorpuffer verbirgt das), `on` = immer, `off` = nie. |
| `font` | *(DejaVu Sans Bold)* | TrueType-Schriftart für Beschriftungen im Framebuffer. |

Die Sprache folgt dem Gebietsschema des Systems (`LANG`), sofern `language` in `[app]` nicht gesetzt ist. Der Kameraton wird über PulseAudio/PipeWire wiedergegeben, sofern verfügbar, andernfalls über ALSA.

### Automatischer Start (systemd)

Für einen Monitor, der die Konsole anzeigt (z. B. ein Raspberry Pi ohne Desktop), kann FrigatePiP als Dienst beim Hochfahren des Computers starten.

**Mit apt installiert (Repository oder `.deb`-Datei):** Der Dienst ist enthalten — aktivieren Sie ihn einfach für Ihren Benutzer (ersetzen Sie `pi` durch Ihren Benutzernamen): `sudo systemctl enable --now frigate-pip@pi`. Sein Protokoll: `journalctl -u frigate-pip@pi`.

**Aus dem Archiv installiert:** Legen Sie `/etc/systemd/system/frigate-pip.service` an (ersetzen Sie `pi` durch Ihren Benutzernamen):

```ini
[Unit]
Description=FrigatePiP
After=network-online.target
Wants=network-online.target

[Service]
User=pi
SupplementaryGroups=video audio
ExecStart=/usr/local/bin/frigate-pip --output framebuffer
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Danach: `sudo systemctl enable --now frigate-pip`. Das Protokoll wird ins Journal (`journalctl -u frigate-pip`) und nach `~/.config/frigate-pip/frigate-pip.log` geschrieben. Damit der blinkende Konsolencursor nicht in den Fenstern erscheint, fügen Sie `vt.global_cursor_default=0` zur Kernel-Befehlszeile hinzu (auf einem Raspberry Pi: `/boot/firmware/cmdline.txt`).

### Fehlerbehebung unter Linux

- **„cannot start ffmpeg“** — installieren Sie ffmpeg (`sudo apt install ffmpeg`) oder setzen Sie `ffmpeg` in `[console]`.
- **„/dev/fb0: Permission denied“** — fügen Sie den Benutzer der Gruppe `video` hinzu und melden Sie sich neu an (für den Dienst: `SupplementaryGroups=video`).
- **Keine Beschriftung im Framebuffer** — installieren Sie `fonts-dejavu-core` oder setzen Sie `font` in `[console]`.
- **Das Bild im Terminal ist grob und blockig** — das ist der Halbblock-Modus; verwenden Sie ein Terminal mit Kitty-Grafik oder Sixel (siehe oben) oder setzen Sie `graphics`.
- **Es erscheint kein tmux-Popup** — Popups erscheinen nur, während Sie in tmux arbeiten, nicht in einem normalen Terminalfenster (siehe [tmux-Popups](#tmux-popups)). Prüfen Sie, ob ein tmux-Client verbunden ist (`tmux list-clients` führt ihn auf), ob `frigate-pip --version` 1.3.0 oder neuer anzeigt, ob tmux in Version 3.2 oder neuer vorliegt (`tmux -V`) und ob der Hintergrundprozess läuft (`tmux ls` zeigt die Sitzung `frigate-pip`); im Protokoll steht, warum ein Popup nicht angezeigt wurde.
- **Blasse Farben in tmux-Popups** — aktivieren Sie 24-Bit-Farbe in tmux (siehe [tmux-Popups](#tmux-popups)).
- **Kein Ton** — der Kameraton benötigt PulseAudio/PipeWire oder ALSA; der Benutzer des Dienstes muss Mitglied der Gruppe `audio` sein.
- **Für einen ausführlichen Bericht** starten Sie FrigatePiP mit `FRIGATEPIP_FFMPEG_LOG=/tmp/ffmpeg.log frigate-pip` und hängen Sie das Ende dieser Datei und des Protokolls an Ihr Issue an.
- **Keine Desktop-Fenster** — sie brauchen X11 oder XWayland: `echo $DISPLAY` muss etwas wie `:0` anzeigen. Ohne das verwendet FrigatePiP die Konsolenmodi; das Protokoll nennt den Grund (`No desktop windows`).
- **FrigatePiP meldet, dass es bereits läuft** — pro Benutzer läuft eine Kopie, zum Beispiel die mit der Desktop-Sitzung gestartete; `frigate-pip --quit` beendet sie.
- **Kein Symbol im Infobereich unter GNOME** — GNOME zeigt solche Symbole nur mit der Erweiterung „AppIndicator and KStatusNotifierItem Support“ (Paket `gnome-shell-extension-appindicator`; in Ubuntu enthalten). Ohne sie starten Sie FrigatePiP erneut aus dem Anwendungsmenü, um die Einstellungen zu öffnen.
