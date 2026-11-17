# Privacybeleid van dMsg

Laatst bijgewerkt: 2026-10-29.

dMsg versleutelt privé-inhoud op je geautoriseerde apparaten. Dit beleid is van toepassing op dmsg.net, de verouderde alleen-lezen webapp, de desktop- en iOS-apps, connect.dmsg.net, de clouddienst en de canisters op het Internet Computer (ICP). Met “wij” bedoelen we het team dat dMsg beheert onder het bestuur van ICPanda DAO. De beschikbaarheid van afzonderlijke functies hangt af van de app-versie en de configuratie van de dienst.

Wij verkopen geen persoonsgegevens en gebruiken je inhoud niet voor advertenties. De website bevat geen advertentie- of analysetrackers.

## 1. Inhoud en openbare informatie

Kluisitems (Vault), privéberichten en bestanden worden versleuteld voordat ze worden geüpload. Inhoudssleutels blijven op geautoriseerde apparaten of in herstel- en apparaatenveloppen die de dienst niet kan openen. Ontvangers kunnen gedeelde kopieën lezen en bewaren. Openbare profielen, avatars, handles (gebruikersnamen), directorydocumenten en informatie die je bewust publiceert, zijn openbaar. Een melding van misbruik is een afzonderlijke bekendmaking: moderators ontvangen je opgegeven reden, toelichting, account- of resource-identificaties en uitsluitend de berichttekst die je expliciet kiest mee te sturen. Door een melding te doen krijgen moderators nooit de sleutels van je account.

## 2. Metagegevens van de dienst

De dienst verwerkt account- en inlog-identificaties, openbare apparaatsleutels, kanaallidmaatschappen, contact- en blokkeringsregels, objectgroottes, tijdstippen van verzoeken, IP-adressen en operationele verzoekgegevens. Gecertificeerde accountstatussen, handtekeningbewijzen en gebruiksrechten (entitlements) stellen client-apps en diensten in staat om bevoegdheden te verifiëren. Ledgertransacties en openbare of gecertificeerde on-chain gegevens kunnen over het algemeen niet worden gewist.

Als je meldingen inschakelt, verwerkt de meldingsdienst ook wat nodig is om meldingen vóór verzending te filteren: ondoorzichtige resource-identificaties, welke kanalen je hebt gedempt of verborgen, je stille uren en tijdzone, weergavebevestigingen (wanneer je een herinnering daadwerkelijk hebt bekeken en tot welk bericht) en welke accounts op dezelfde installatie zijn geregistreerd. Voor deadlineherinneringen over een openstaand accountherstel uploadt je apparaat het herstelrecord, inclusief de nieuwe inlogidentiteit en de openbare sleutel van het apparaat; de dienst controleert het en bewaart alleen de herinnering en het uitvoeringstijdstip. De dienst ontvangt geen kanaalnamen, berichtinhoud of sleutels om die te ontsleutelen. Het intrekken van de registratie, het verwijderen van het account van het apparaat of het verwijderen van het serviceaccount wist deze voorkeuren, weergavebevestigingen en installatiekoppelingen; bezorg- en herhaalgegevens worden alleen bewaard tot ze verlopen.

## 3. Apple-diensten en betalingen

Op iOS maken optionele meldingen gebruik van de Apple Push Notification-service (APNs). Apple en onze bezorgdienst verwerken een installatie-identificatie, apparaattoken en bezorgmetagegevens. Meldingen bevatten een algemene herinnering en een ondoorzichtige route, nooit een berichttekst, wachtwoord of bestandsnaam. Je kunt meldingen uitschakelen in dMsg of in de systeeminstellingen; reeds afgeleverde meldingen kunnen niet altijd worden ingetrokken. Als meerdere accounts meldingen ontvangen op één installatie, kunnen Apple en onze bezorgdienst ze via het gedeelde apparaattoken aan elkaar koppelen; dMsg vraagt om je toestemming voordat meldingen voor een ander account worden ingeschakeld.

App Attest verwerkt een door de app gegenereerde sleutelidentificatie, attestatie-/assertiebewijzen en cryptografische uitdagingen (challenges) om een app-installatie te verifiëren. Face ID, Touch ID en de toegangscode van het apparaat worden afgehandeld door het besturingssysteem; dMsg ontvangt geen biometrische gegevens of je toegangscode.

StoreKit-aankopen worden afgehandeld door Apple. Onze verificatiedienst en commerce-canister verwerken transactie- en product-identificaties, een accountbindend token, aankoop- en vervaldatums en de terugbetalings- of intrekkingsstatus. Zij ontvangen geen betaalkaartgegevens. Andere dMsg-clients ondersteunen mogelijk betalingen via ICP-wallets en PANDA-lidmaatschap, met openbare ledger- en rechtenregistraties. Het verwijderen van je dMsg-account zegt een Apple-abonnement niet op; beheer je abonnementen in je Apple Account.

Het beleid en de voorstellen van multisig-wallets worden verwerkt door de wallet-canisters; hun zichtbaarheid volgt de instellingen van de betreffende wallet. Blockchain-RPC-providers ontvangen de adressen, transactiegegevens en IP-informatie die nodig zijn voor saldocontroles, kostenberekeningen en het uitzenden (broadcasten) van transacties. Standaard-eindpunten zijn onder meer mempool.space, PublicNode, mainnet.base.org en api.mainnet-beta.solana.com; zelf geconfigureerde alternatieven hanteren hun eigen privacybeleid.

## 4. Lokale gegevens en systeemintegraties

De native apps bewaren een lokaal versleutelde werkruimte, door het besturingssysteem beschermde sleutels (waar ondersteund), concepten, wachtende bewerkingen en voorkeuren. Door te ontgrendelen wordt de geselecteerde inhoud beschikbaar in het werkgeheugen van de app. Geëxporteerde bestanden en gekopieerde tekst kunnen door de ontvangende apps worden bewaard.

De Deel-extensie (Share) van iOS zet alleen de items klaar die je selecteert voor een latere import in een account. De integraties voor Automatisch invullen van wachtwoorden (AutoFill) en de Bestanden-app bevatten uitsluitend expliciet geselecteerde, afzonderlijk versleutelde momentopnamen (snapshots). Automatisch invullen vereist onafhankelijke systeemauthenticatie; momentopnamen verlopen na 24 uur. Toegangssessies voor de Bestanden-app duren maximaal vijf minuten en eindigen zodra dMsg wordt vergrendeld, maar de Bestanden-app en andere ontvangers kunnen reeds gedownloade kopieën behouden. Het wijzigen van bronitems werkt een eerder geëxporteerde momentopname niet automatisch bij.

Expliciet gestarte achtergrondoverdrachten slaan uitsluitend begrensde versleutelde gegevens (ciphertext) en een overdrachtsbevoegdheid op in een aparte apparaatmap die toegankelijk is na de eerste ontgrendeling van het apparaat. Ze bevatten geen sleutels voor accountondertekening of inhoud. Het oorspronkelijke account moet worden ontgrendeld om een overdracht te verifiëren en af te ronden. Het besturingssysteem kan de aflevering vertragen of stopzetten.

De website onthoudt je taalkeuze lokaal. De verouderde webapp gebruikt browseropslag voor sessies en gecachte gegevens; de walletweergave daarvan kan prijzen opvragen bij ICPSwap. Het verwijderen van een app of het wissen van de app-gegevens kan leiden tot permanent verlies van inhoud die nog nooit is gesynchroniseerd of geëxporteerd.

## 5. Dienstverleners en bewaartermijnen

Cloudflare verwerkt versleutelde cloudgegevens en verzoekmetagegevens. ICP-nodeproviders en gateways exploiteren canisters en websites. Apple distribueert de iOS-app en levert de hierboven beschreven optionele diensten. Internet Identity, walletproviders en gekoppelde applicaties hebben hun eigen privacybeleid. Wij kunnen informatie waarover wij beschikken openbaar maken wanneer dat wettelijk verplicht is, waaronder expliciet ingediende meldingen; wij kunnen geen onversleutelde tekst (plaintext) verstrekken die wij niet bezitten.

Versleutelde gegevens in de cloud volgen de opslag- en verwijderingsregels van het account. Het verwijderen van een serviceaccount is een traceerbaar opschoningsverzoek: het blokkeert nieuwe toegang tot de dienst, verwijdert cloudinhoud van het account en openbare avatarkopieën, en meldt pas voltooiing nadat de opschoning is afgerond. Het wist geen gedeelde kopieën bij anderen, lokale kopieën op andere apparaten, de openbare ledgergeschiedenis of noodzakelijke financiële en anti-replaygegevens. Openstaande transacties of actieve kanalen waarvan je eigenaar bent, moeten mogelijk eerst worden afgewikkeld. Gegevens van misbruikmeldingen worden maximaal 90 dagen bewaard voor beoordeling; bezorgtokens verlopen tenzij ze worden vernieuwd. Operationele logboeken worden alleen bewaard voor zover nodig voor beveiliging, misbruikpreventie en probleemoplossing.

## 6. Keuzes, rechten en contact

Je kunt inhoud exporteren of verwijderen, apparaten of browsers intrekken, afzenders blokkeren, misbruik melden, contactregels wijzigen, verwijdering van je serviceaccount aanvragen en afzonderlijk de accountgegevens op dit apparaat wissen. Afhankelijk van je woonplaats heb je mogelijk het recht op inzage, rectificatie, verwijdering of bezwaar tegen de verwerking van persoonsgegevens. Neem contact op met [Ondersteuning](https://dmsg.net/_/support) voor verzoeken of vertrouwelijke meldingen van kwetsbaarheden. Wij leggen uit wanneer versleuteling, wettelijk bewaarde transactiegegevens of onveranderlijke openbare registers een verzoek beperken.

Geen enkel systeem is volmaakt veilig. App-updates en canister-upgrades onder DAO-bestuur blijven vertrouwenspunten. dMsg is niet bedoeld voor kinderen jonger dan 13 jaar of onder de geldende minimumleeftijd voor het gebruik van onlinediensten zonder ouderlijke toestemming. Bij wezenlijke wijzigingen in dit beleid werken we bovenstaande datum bij en kondigen we de wijzigingen aan in de app of op de website.
