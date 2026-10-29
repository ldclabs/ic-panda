# dMsg Datenschutzrichtlinie

Letzte Aktualisierung: 29.10.2026.

dMsg verschlüsselt private Inhalte auf Ihren autorisierten Geräten. Diese Richtlinie gilt für dmsg.net, die schreibgeschützte Legacy-Web-App, die Desktop- und iOS-Apps, connect.dmsg.net, den Cloud-Dienst und den Internet-Computer (ICP) canisters. „Wir“ bezeichnet das Team, das dMsg unter der Leitung von ICPanda DAO betreibt. Die Verfügbarkeit einzelner Funktionen hängt von der App-Version und der Dienstkonfiguration ab.

Wir verkaufen keine personenbezogenen Daten und nutzen Ihre Inhalte nicht für Werbung. Die Website enthält keine Werbe- oder Analyse-Tracker.

## 1. Inhalt und öffentliche Informationen

Tresorelemente, private Nachrichten und Dateien werden vor dem Hochladen verschlüsselt. Inhaltsschlüssel verbleiben auf autorisierten Geräten oder in Wiederherstellungs-/Geräteumschlägen, die der Dienst nicht öffnen kann. Empfänger können freigegebene Kopien lesen und behalten. Öffentliche Profile, Avatare, Benutzernamen, Verzeichnisdokumente und Informationen, die Sie absichtlich veröffentlichen, sind öffentlich. Ein Bericht ist eine separate Offenlegung: Moderatoren erhalten Ihren Grund, Ihre Notiz, Ihre Konto-/Ressourcen-IDs und nur den Nachrichtentext, den Sie ausdrücklich angeben möchten. Durch die Berichterstattung erhalten Moderatoren nicht die Schlüssel zu Ihrem Konto.

## 2. Dienstmetadaten

Der Dienst verarbeitet Konto- und Anmeldekennungen, öffentliche Geräteschlüssel, Kanalmitgliedschaft, Kontakt- und Blockierungsregeln, Objektgrößen, Anforderungszeitpunkt, IP-Adressen und betriebliche Anforderungsinformationen. Beglaubigter Kontostatus, Unterschriftsbelege und Berechtigungen ermöglichen es Kunden und Diensten, die Berechtigung zu überprüfen. Ledger-Transaktionen und öffentliche oder zertifizierte On-Chain-Aufzeichnungen können im Allgemeinen nicht gelöscht werden.

Wenn Sie Benachrichtigungen aktivieren, verarbeitet der Benachrichtigungsdienst zusätzlich, was er zum Filtern von Hinweisen vor dem Versand benötigt: undurchsichtige Ressourcenkennungen, welche Kanäle Sie stummgeschaltet oder ausgeblendet haben, Ruhezeiten und Zeitzone, Ansichtsbestätigungen (wann Sie eine Erinnerung tatsächlich angesehen haben und bis zu welcher Nachricht) sowie welche Konten auf derselben Installation registriert sind. Für Fristerinnerungen zu einer ausstehenden Kontowiederherstellung lädt Ihr Gerät den Wiederherstellungsdatensatz hoch, einschließlich der neuen Anmeldeidentität und des öffentlichen Geräteschlüssels; der Dienst prüft ihn und speichert nur die Erinnerung und ihren Ausführungszeitpunkt. Der Dienst erhält keine Kanalnamen, keine Nachrichteninhalte und keine Schlüssel zu deren Entschlüsselung. Das Widerrufen der Registrierung, das Entfernen des Kontos vom Gerät oder das Löschen des Dienstkontos löscht diese Einstellungen, Ansichtsbestätigungen und Installationsverknüpfungen; Zustell- und Wiederholungsdatensätze werden nur bis zu ihrem Ablauf aufbewahrt.

## 3. Apple-Dienste und Zahlungen

Unter iOS nutzen optionale Benachrichtigungen den Apple Push Notification Service. Apple und unser Lieferservice verarbeiten eine Installationskennung, ein Geräte-Token und Liefermetadaten. Benachrichtigungen enthalten eine allgemeine Erinnerung und eine undurchsichtige Route, keinen Nachrichtentext, kein Passwort oder Dateinamen. Sie können Benachrichtigungen in dMsg oder in den Systemeinstellungen deaktivieren; Bereits zugestellte Benachrichtigungen können nicht immer zurückgerufen werden. Erhalten mehrere Konten Benachrichtigungen auf derselben Installation, können Apple und unser Zustelldienst sie über das gemeinsame Geräte-Token verknüpfen; dMsg fragt nach, bevor Benachrichtigungen für ein weiteres Konto aktiviert werden.

App Attest verarbeitet eine von der App generierte Schlüsselkennung, Attestierungs-/Behauptungsnachweise und Herausforderungen, um eine App-Installation zu überprüfen. Face ID, Touch ID und der Gerätepasscode werden vom Betriebssystem verwaltet; dMsg empfängt keine biometrischen Bilder oder Ihren Passcode.

StoreKit-Käufe werden von Apple abgewickelt. Unser Verifizierer und Handelsunternehmen canister verarbeitet Transaktions-/Produktkennungen, ein kontobindendes Token, Kauf- und Ablaufdaten sowie den Rückerstattungs-/Widerrufsstatus. Sie erhalten Ihre Zahlungskartendaten nicht. Andere dMsg-Clients unterstützen möglicherweise ICP-Wallet-Zahlungen und die PANDA-Mitgliedschaft mit öffentlichen Hauptbuch- und Berechtigungsdatensätzen. Durch die Kontolöschung wird ein Apple-Abonnement nicht gekündigt; Verwalten Sie Abonnements in Ihrem Apple-Konto.

Team-Wallet-Richtlinien und -Vorschläge werden von der Wallet canisters verarbeitet; Ihre Sichtbarkeit richtet sich nach den Einstellungen des Wallets. Blockchain RPC-Anbieter erhalten die Adressen, Transaktionsdaten und IP-Informationen, die für Kontostandabfragen, Gebührensimulation und Ausstrahlung benötigt werden. Zu den Standardendpunkten gehören mempool.space, PublicNode, mainnet.base.org und api.mainnet-beta.solana.com; konfigurierte Alternativen haben ihre eigenen Datenschutzpraktiken.

## 4. Lokale Daten- und Systemintegrationen

Die nativen Apps verfügen über einen lokalen, verschlüsselten Arbeitsbereich, vom Betriebssystem geschützte Schlüssel (sofern unterstützt), Entwürfe, ausstehende Vorgänge und Einstellungen. Durch das Entsperren werden ausgewählte Inhalte im App-Speicher verfügbar. Exportierte Dateien und kopierter Text können von ihren Ziel-Apps beibehalten werden.

Die iOS-Share-Erweiterung stellt nur die Elemente bereit, die Sie für einen späteren Kontoimport auswählen. Passwort-AutoFill- und Dateiintegrationen enthalten nur explizit ausgewählte, separat verschlüsselte Snapshots. AutoFill erfordert eine unabhängige Systemauthentifizierung; Schnappschüsse verfallen nach 24 Stunden. Dateizugriffssitzungen dauern höchstens fünf Minuten und enden, wenn dMsg gesperrt wird. Die Dateien-App und andere Empfänger behalten jedoch möglicherweise bereits heruntergeladene Kopien. Durch das Ändern von Quellelementen wird ein exportierter Snapshot nicht automatisch aktualisiert.

Bei expliziten Hintergrundübertragungen werden nur begrenzter Chiffretext und Übertragungsberechtigungen in einem separaten Geräteverzeichnis gespeichert, auf das nach dem ersten Entsperren des Geräts zugegriffen werden kann. Sie verfügen nicht über Kontosignatur- oder Inhaltsschlüssel. Das ursprüngliche Konto muss entsperrt werden, um eine Übertragung zu überprüfen und abzuschließen. Das System kann die Lieferung verzögern oder stoppen.

Die Website merkt sich Ihre Sprache lokal. Die Legacy-App verwendet Browserspeicher für Sitzungen und zwischengespeicherte Daten. seine Wallet-Ansicht kann Preise von ICPSwap anfordern. Durch das Entfernen einer App oder das Löschen ihrer Daten können Inhalte, die nie synchronisiert oder exportiert wurden, dauerhaft verloren gehen.

## 5. Anbieter und Datenspeicherung

Cloudflare verarbeitet Cloud-Chiffretext und fordert Metadaten an. ICP-Knotenanbieter und Gateways betreiben canisters und Websites. Apple vertreibt die iOS-App und stellt die oben beschriebenen optionalen Dienste bereit. Internet Identity, Wallet-Anbieter und verbundene Anwendungen haben ihre eigenen Datenschutzrichtlinien. Wir können die von uns gespeicherten Informationen offenlegen, wenn dies gesetzlich vorgeschrieben ist, einschließlich ausdrücklich eingereichter Berichte; Wir können keinen Klartext offenlegen, den wir nicht besitzen.

Cloud-Chiffretext folgt den Speicher- und Löschregeln des Kontos. Bei der Löschung eines Dienstkontos handelt es sich um eine nachverfolgte Bereinigungsanforderung: Sie blockiert den Zugriff auf neue Dienste, entfernt kontoeigene Cloud-Inhalte und öffentliche Avatar-Kopien und meldet den Abschluss erst nach der Bereinigung. Es löscht nicht die freigegebenen Kopien anderer Personen, lokale Kopien auf anderen Geräten, den Verlauf des öffentlichen Hauptbuchs oder notwendige Finanz-/Anti-Replay-Aufzeichnungen. Nicht abgewickelte Transaktionen oder eigene aktive Kanäle müssen möglicherweise zunächst geklärt werden. Berichtsdatensätze werden zur Überprüfung bis zu 90 Tage lang aufbewahrt; Liefertoken verfallen, sofern sie nicht erneuert werden. Betriebsprotokolle werden nur bei Bedarf zur Sicherheit, Missbrauchsprävention und Fehlerbehebung geführt.

## 6. Wahlmöglichkeiten, Rechte und Kontakt

Sie können Inhalte exportieren oder löschen, Geräte oder Browser sperren, Absender blockieren, Missbrauch melden, Kontaktregeln ändern, die Löschung eines Dienstkontos beantragen und die Kontodaten dieses Geräts separat entfernen. Abhängig von Ihrem Standort haben Sie möglicherweise das Recht, auf personenbezogene Daten zuzugreifen, sie zu korrigieren, zu löschen oder der Verarbeitung zu widersprechen. Kontaktieren Sie [Support](https://dmsg.net/_/support) für Anfragen oder private Schwachstellenberichte. Wir erklären, wann Verschlüsselung, aufbewahrte Transaktionsaufzeichnungen oder unveränderliche öffentliche Aufzeichnungen eine Anfrage einschränken.

Kein System ist vollkommen sicher. App-Updates und canister-Upgrades unter der DAO-Governance bleiben Vertrauenspunkte. dMsg ist nicht für Kinder unter 13 Jahren oder unter dem geltenden Mindestalter für die Nutzung von Online-Diensten ohne Zustimmung der Eltern bestimmt. Wir aktualisieren das oben genannte Datum und geben wesentliche Richtlinienänderungen in der App oder auf der Website bekannt.
