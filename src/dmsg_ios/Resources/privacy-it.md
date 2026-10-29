# dMsg Informativa sulla privacy

Ultimo aggiornamento: 29-10-2026.

dMsg crittografa i contenuti privati sui tuoi dispositivi autorizzati. Questa policy copre dmsg.net, l'app Web legacy di sola lettura, le app desktop e iOS, connect.dmsg.net, il servizio cloud e il computer Internet (ICP) canisters. "Noi" indica il team che opera dMsg sotto la governance ICPanda DAO. La disponibilità delle singole funzionalità dipende dalla versione dell'app e dalla configurazione del servizio.

Non vendiamo dati personali né utilizziamo i tuoi contenuti per la pubblicità. Il sito web non contiene tracker pubblicitari o analitici.

## 1. Contenuti e informazioni pubbliche

Gli oggetti del vault, i messaggi privati e i file vengono crittografati prima del caricamento. Le chiavi dei contenuti rimangono sui dispositivi autorizzati o in buste di ripristino/dispositivo che il servizio non può aprire. I destinatari possono leggere e conservare copie condivise. Profili pubblici, avatar, handle, documenti di directory e informazioni che pubblichi intenzionalmente sono pubblici. Una segnalazione è un'informativa separata: i moderatori ricevono il motivo, la nota, gli identificatori dell'account/risorsa e solo il testo del messaggio che scegli esplicitamente di includere. La segnalazione non fornisce ai moderatori le chiavi del tuo account.

## 2. Metadati del servizio

Il servizio elabora identificatori di account e accesso, chiavi pubbliche del dispositivo, appartenenza al canale, regole di contatto e blocco, dimensioni degli oggetti, tempistica delle richieste, indirizzi IP e informazioni sulle richieste operative. Lo stato certificato dell'account, le ricevute delle firme e i diritti consentono ai clienti e ai servizi di verificare l'autorità. Le transazioni del registro e i record on-chain pubblici o certificati generalmente non possono essere cancellati.

Se attivi le notifiche, il servizio di notifica elabora anche ciò che serve per filtrare gli avvisi prima dell’invio: identificatori opachi delle risorse, i canali che hai silenziato o nascosto, la fascia silenziosa e il fuso orario, le conferme di visualizzazione (quando hai effettivamente visto un promemoria e fino a quale messaggio) e quali account sono registrati sulla stessa installazione. Per i promemoria di scadenza di un recupero dell’account in attesa, il tuo dispositivo carica il record di recupero, inclusi la nuova identità di accesso e la chiave pubblica del dispositivo; il servizio lo verifica e conserva solo il promemoria e l’orario di esecuzione. Il servizio non riceve nomi dei canali, contenuto dei messaggi né chiavi per decifrarli. Revocare la registrazione, rimuovere l’account dal dispositivo o eliminare l’account del servizio cancella queste preferenze, le conferme di visualizzazione e i collegamenti all’installazione; i record di consegna e di nuovo tentativo sono conservati solo fino alla loro scadenza.

## 3. Servizi e pagamenti Apple

Su iOS, le notifiche opzionali utilizzano il servizio di notifica push di Apple. Apple e il nostro servizio di consegna elaborano un identificatore di installazione, un token del dispositivo e metadati di consegna. Le notifiche contengono un promemoria generico e un percorso opaco, non il corpo del messaggio, una password o un nome file. È possibile disattivare le notifiche in dMsg o nelle impostazioni di sistema; non sempre è possibile richiamare le notifiche già consegnate. Se più account ricevono notifiche sulla stessa installazione, il token del dispositivo condiviso consente ad Apple e al nostro servizio di consegna di collegarli; dMsg chiede il consenso prima di attivare le notifiche per un altro account.

App Attest elabora un identificatore di chiave generato dall'app, prove di attestazione/asserzione e sfide per verificare l'installazione di un'app. Face ID, Touch ID e il codice di accesso del dispositivo sono gestiti dal sistema operativo; dMsg non riceve immagini biometriche o il tuo passcode.

Gli acquisti di StoreKit sono gestiti da Apple. Il nostro verificatore ed e-commerce canister elabora gli identificatori di transazione/prodotto, un token vincolante per l'account, le date di acquisto e di scadenza e lo stato di rimborso/revoca. Non ricevono i dettagli della tua carta di pagamento. Altri client dMsg possono supportare i pagamenti tramite portafoglio ICP e l'iscrizione PANDA, con registro pubblico e record dei diritti. La cancellazione dell'account non annulla un abbonamento Apple; gestire gli abbonamenti nel tuo account Apple.

Le politiche e le proposte del portafoglio del team vengono elaborate dal portafoglio canisters; la loro visibilità segue le impostazioni del portafoglio. I fornitori di Blockchain RPC ricevono gli indirizzi, i dati delle transazioni e le informazioni IP necessarie per le richieste di saldo, la simulazione delle commissioni e la trasmissione. Gli endpoint predefiniti includono mempool.space, PublicNode, mainnet.base.org e api.mainnet-beta.solana.com; le alternative configurate hanno le proprie pratiche sulla privacy.

## 4. Dati locali e integrazioni di sistema

Le app native mantengono uno spazio di lavoro locale crittografato, chiavi protette dal sistema operativo ove supportato, bozze, operazioni pendenti e preferenze. Lo sblocco rende disponibili i contenuti selezionati nella memoria dell'app. I file esportati e il testo copiato potrebbero essere conservati dalle relative app di destinazione.

L'estensione Condividi iOS mette in scena solo gli elementi selezionati per una successiva importazione dell'account. Le integrazioni di Compilazione automatica password e File contengono solo snapshot crittografati separatamente, selezionati esplicitamente. La compilazione automatica richiede l'autenticazione del sistema indipendente; gli snapshot scadono dopo 24 ore. Le sessioni di accesso ai file durano al massimo cinque minuti e terminano quando dMsg si blocca, ma l'app File e altri destinatari potrebbero conservare copie già scaricate. La modifica degli elementi di origine non aggiorna automaticamente uno snapshot esportato.

I trasferimenti espliciti in background archiviano solo il testo cifrato limitato e l'autorità di trasferimento in una directory del dispositivo separata accessibile dopo il primo sblocco del dispositivo. Non detengono la firma dell'account o le chiavi dei contenuti. L'account originale deve essere sbloccato per verificare e completare un trasferimento. Il sistema potrebbe ritardare o interrompere la consegna.

Il sito web ricorda la tua lingua a livello locale. L'app legacy utilizza l'archiviazione del browser per sessioni e dati memorizzati nella cache; la sua visualizzazione del portafoglio può richiedere prezzi da ICPSwap. La rimozione di un'app o la cancellazione dei relativi dati può comportare la perdita permanente di contenuti che non sono mai stati sincronizzati o esportati.

## 5. Fornitori e conservazione dei dati

Cloudflare elabora il testo cifrato cloud e richiede metadati. I provider di nodi e gateway ICP gestiscono canisters e siti Web. Apple distribuisce l'app iOS e fornisce i servizi opzionali sopra descritti. Internet Identity, i fornitori di portafogli e le applicazioni connesse hanno le proprie politiche sulla privacy. Potremmo divulgare le informazioni in nostro possesso quando richiesto dalla legge, comprese le relazioni inviate esplicitamente; non possiamo rivelare testo in chiaro che non possediamo.

Il testo cifrato cloud segue le regole di archiviazione ed eliminazione dell'account. L'eliminazione dell'account di servizio è una richiesta di pulizia tracciata: blocca il nuovo accesso al servizio, rimuove i contenuti cloud di proprietà dell'account e le copie degli avatar pubblici e segnala il completamento solo dopo la pulizia. Non cancella le copie condivise di altre persone, le copie locali su altri dispositivi, la cronologia dei registri pubblici o i necessari record finanziari/anti-replay. Le transazioni non regolate o i canali attivi di proprietà potrebbero richiedere prima una risoluzione. I record dei report vengono conservati per un massimo di 90 giorni per la revisione; i token di consegna scadono se non rinnovati. I registri operativi vengono conservati solo se necessario per la sicurezza, la prevenzione degli abusi e la risoluzione dei problemi.

## 6. Scelte, diritti e contatti

Puoi esportare o eliminare contenuti, revocare dispositivi o browser, bloccare mittenti, segnalare abusi, modificare regole di contatto, richiedere l'eliminazione dell'account di servizio e rimuovere separatamente i dati dell'account di questo dispositivo. A seconda della tua posizione, potresti avere i diritti di accesso, correzione, cancellazione o opposizione al trattamento delle informazioni personali. Contatta [Supporto](https://dmsg.net/_/support) per richieste o rapporti privati ​​sulle vulnerabilità. Spieghiamo quando la crittografia, i record delle transazioni conservati o i record pubblici immutabili limitano una richiesta.

Nessun sistema è perfettamente sicuro. Gli aggiornamenti delle app e gli upgrade canister sotto la governance DAO rimangono punti di fiducia. dMsg non è destinato ai bambini di età inferiore a 13 anni o al di sotto dell'età minima applicabile per l'utilizzo dei servizi online senza il consenso dei genitori. Aggiorniamo la data sopra e annunciamo modifiche sostanziali alle politiche nell'app o sul sito web.
