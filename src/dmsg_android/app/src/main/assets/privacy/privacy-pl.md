# Polityka prywatności dMsg

Ostatnia aktualizacja: 2026-10-29.

dMsg szyfruje prywatne treści na Twoich autoryzowanych urządzeniach. Niniejsza polityka obejmuje witrynę dmsg.net, starszą aplikację internetową działającą w trybie tylko do odczytu, aplikacje na komputery oraz system iOS, stronę connect.dmsg.net, usługę chmurową oraz canistery w sieci Internet Computer (ICP). „My” oznacza zespół prowadzący dMsg w ramach zarządzania ICPanda DAO. Dostępność poszczególnych funkcji zależy od wersji aplikacji i konfiguracji usługi.

Nie sprzedajemy danych osobowych ani nie wykorzystujemy Twoich treści do celów reklamowych. Witryna nie zawiera żadnych skryptów śledzących o charakterze reklamowym ani analitycznym.

## 1. Treści i informacje publiczne

Elementy sejfu (Vault), prywatne wiadomości oraz pliki są szyfrowane przed przesłaniem. Klucze do treści pozostają na autoryzowanych urządzeniach lub w kopertach odzyskiwania/urządzenia, których usługa nie jest w stanie otworzyć. Odbiorcy mogą odczytywać i zachowywać udostępnione im kopie. Profile publiczne, awatary, nazwy użytkownika (handle), dokumenty katalogowe oraz informacje, które celowo publikujesz, mają charakter publiczny. Zgłoszenie nadużycia stanowi odrębne ujawnienie danych: moderatorzy otrzymują podany powód, notatkę, identyfikatory konta/zasobu oraz wyłącznie tę treść wiadomości, którą wyraźnie zdecydujesz się załączyć. Zgłoszenie nie przekazuje moderatorom kluczy do Twojego konta.

## 2. Metadane usługi

Usługa przetwarza identyfikatory kont i logowania, klucze publiczne urządzeń, przynależność do kanałów, reguły kontaktów i blokowania, rozmiary obiektów, czas zapytań, adresy IP oraz informacje operacyjne dotyczące żądań. Certyfikowany stan konta, potwierdzenia podpisów i uprawnienia (entitlements) pozwalają aplikacjom klienckim oraz usługom weryfikować autoryzację. Transakcje w księgach (ledgerach) oraz publiczne lub certyfikowane wpisy on-chain z reguły nie mogą zostać usunięte.

Jeśli włączysz powiadomienia, usługa powiadomień przetwarza również dane niezbędne do odfiltrowania alertów przed ich wysłaniem: nieprzejrzyste identyfikatory zasobów, wyciszone lub ukryte kanały, godziny ciszy i strefę czasową, potwierdzenia wyświetlenia (kiedy faktycznie wyświetlono przypomnienie i do której wiadomości) oraz informację o tym, które konta są zarejestrowane w ramach tej samej instalacji. W przypadku przypomnień o terminie oczekującego odzyskiwania konta Twoje urządzenie przesyła rekord odzyskiwania, w tym nową tożsamość logowania i klucz publiczny urządzenia; usługa weryfikuje go i zachowuje wyłącznie przypomnienie oraz czas wykonania. Usługa nie otrzymuje nazw kanałów, treści wiadomości ani kluczy pozwalających na ich odszyfrowanie. Cofnięcie rejestracji, usunięcie konta z urządzenia lub usunięcie konta w usłudze powoduje wyczyszczenie tych preferencji, potwierdzeń wyświetlenia i powiązań instalacji; zapisy dotyczące doręczeń i ponownych prób są przechowywane tylko do czasu ich wygaśnięcia.

## 3. Usługi Apple i płatności

W systemie iOS opcjonalne powiadomienia korzystają z usługi Apple Push Notification service (APNs). Apple oraz nasza usługa doręczania przetwarzają identyfikator instalacji, token urządzenia oraz metadane doręczenia. Powiadomienia zawierają ogólne przypomnienie i nieprzejrzystą ścieżkę docelową, a nie treść wiadomości, hasło czy nazwę pliku. Powiadomienia można wyłączyć w dMsg lub w ustawieniach systemowych; powiadomień już dostarczonych nie zawsze da się wycofać. Jeśli w ramach jednej instalacji powiadomienia odbiera kilka kont, współdzielony token urządzenia pozwala Apple i naszej usłudze doręczania powiązać je ze sobą; dMsg prosi o zgodę przed włączeniem powiadomień dla kolejnego konta.

Usługa App Attest przetwarza wygenerowany przez aplikację identyfikator klucza, dowody poświadczenia/asercji oraz wyzwania kryptograficzne (challenges) w celu zweryfikowania autentyczności instalacji aplikacji. Face ID, Touch ID oraz kod blokady urządzenia są obsługiwane przez system operacyjny; dMsg nie otrzymuje obrazów biometrycznych ani Twojego kodu.

Zakupy StoreKit są obsługiwane przez Apple. Nasz weryfikator oraz canister handlowy (commerce canister) przetwarzają identyfikatory transakcji i produktów, token wiążący z kontem, daty zakupu i wygaśnięcia oraz status zwrotu lub cofnięcia uprawnień. Nie otrzymują one danych Twojej karty płatniczej. Inne aplikacje klienckie dMsg mogą obsługiwać płatności z portfela ICP oraz członkostwo PANDA, wiążące się z publicznymi wpisami w księdze i rejestrze uprawnień. Usunięcie konta dMsg nie anuluje subskrypcji Apple – subskrypcjami należy zarządzać na koncie Apple Account.

Polityki i propozycje portfeli wielopodpisowych (multisig) są przetwarzane przez canistery portfela, a ich widoczność zależy od ustawień danego portfela. Dostawcy węzłów RPC blockchain otrzymują adresy, dane transakcji oraz informacje o adresie IP niezbędne do sprawdzania salda, symulacji opłat i rozgłaszania (broadcastingu) transakcji. Domyślne punkty końcowe obejmują mempool.space, PublicNode, mainnet.base.org oraz api.mainnet-beta.solana.com; skonfigurowane samodzielnie alternatywy stosują własne zasady prywatności.

## 4. Dane lokalne i integracje systemowe

Aplikacje natywne przechowują lokalną zaszyfrowaną przestrzeń roboczą, klucze chronione przez system operacyjny (tam, gdzie jest to obsługiwane), wersje robocze, oczekujące operacje oraz preferencje. Odblokowanie aplikacji udostępnia wybrane treści w jej pamięci operacyjnej. Wyeksportowane pliki i skopiowany tekst mogą zostać zachowane przez aplikacje docelowe.

Rozszerzenie udostępniania (Share) w systemie iOS jedynie przygotowuje wybrane przez Ciebie elementy do późniejszego zaimportowania na konto. Integracje z funkcjami Autouzupełniania haseł (AutoFill) oraz aplikacją Pliki (Files) zawierają wyłącznie wyraźnie wybrane, osobno zaszyfrowane migawki (snapshots). Autouzupełnianie wymaga niezależnego uwierzytelnienia systemowego; migawki wygasają po 24 godzinach. Sesje dostępu do aplikacji Pliki trwają maksymalnie pięć minut i kończą się z chwilą zablokowania dMsg, jednak aplikacja Pliki oraz inni odbiorcy mogą zachować wcześniej pobrane kopie. Zmiana elementów źródłowych nie aktualizuje automatycznie wyeksportowanej migawki.

Jawnie uruchomione transfery w tle zapisują wyłącznie ograniczony szyfrogram oraz upoważnienie do transferu w osobnym katalogu urządzenia, dostępnym po pierwszym odblokowaniu urządzenia. Nie przechowują one kluczy podpisu konta ani kluczy do treści. Aby zweryfikować i sfinalizować transfer, pierwotne konto musi zostać odblokowane. System operacyjny może opóźnić lub przerwać dostarczanie.

Witryna zapamiętuje wybrany język lokalnie w przeglądarce. Starsza aplikacja internetowa wykorzystuje pamięć przeglądarki do przechowywania sesji i danych podręcznych (cache), a jej widok portfela może pobierać kursy z ICPSwap. Usunięcie aplikacji lub wyczyszczenie jej danych może spowodować bezpowrotną utratę treści, które nigdy nie zostały zsynchronizowane ani wyeksportowane.

## 5. Dostawcy usług i okres przechowywania danych

Cloudflare przetwarza szyfrogramy chmurowe oraz metadane zapytań. Dostawcy węzłów i bram sieci ICP utrzymują canistery oraz strony internetowe. Apple dystrybuuje aplikację iOS i świadczy opisane wyżej usługi opcjonalne. Internet Identity, dostawcy portfeli oraz połączone aplikacje stosują własne polityki prywatności. Możemy ujawnić posiadane przez nas informacje, gdy wymaga tego prawo, w tym wyraźnie przesłane zgłoszenia nadużyć; nie jesteśmy w stanie ujawnić tekstu jawnego (plaintext), którego nie posiadamy.

Szyfrogramy w chmurze podlegają regułom przechowywania i usuwania przypisanym do konta. Usunięcie konta w usłudze to monitorowane żądanie czyszczenia: blokuje ono nowy dostęp do usługi, usuwa należące do konta treści w chmurze oraz publiczne kopie awatara i potwierdza zakończenie dopiero po wykonaniu czyszczenia. Operacja ta nie usuwa kopii udostępnionych innym osobom, lokalnych kopii na innych urządzeniach, publicznej historii księgi (ledgera) ani niezbędnych zapisów finansowych i zabezpieczających przed powtórzeniem transakcji (anti-replay). Nierozliczone transakcje lub posiadane aktywne kanały mogą wymagać wcześniejszego zamknięcia. Zgłoszenia nadużyć są przechowywane do weryfikacji maksymalnie przez 90 dni; tokeny doręczeń wygasają, o ile nie zostaną odnowione. Dzienniki operacyjne (logi) są zachowywane wyłącznie w zakresie niezbędnym do zapewnienia bezpieczeństwa, zapobiegania nadużyciom i diagnostyki błędów.

## 6. Twoje wybory, prawa i kontakt

Możesz eksportować lub usuwać treści, cofać autoryzację urządzeń lub przeglądarek, blokować nadawców, zgłaszać nadużycia, zmieniać reguły kontaktu, zażądać usunięcia konta w usłudze oraz niezależnie usunąć dane konta z tego urządzenia. W zależności od miejsca zamieszkania może przysługiwać Ci prawo dostępu do danych osobowych, ich sprostowania, usunięcia lub wniesienia sprzeciwu wobec ich przetwarzania. W sprawie wniosków lub poufnych zgłoszeń luk w zabezpieczeniach skontaktuj się ze [Wsparciem](https://dmsg.net/_/support). Wyjaśnimy sytuacje, w których szyfrowanie, obowiązkowo przechowywane zapisy transakcji lub niezmienne rejestry publiczne ograniczają możliwość realizacji żądania.

Żaden system nie zapewnia absolutnego bezpieczeństwa. Aktualizacje aplikacji oraz aktualizacje canisterów w ramach zarządzania DAO pozostają elementami opartymi na zaufaniu. Usługa dMsg nie jest przeznaczona dla dzieci poniżej 13. roku życia ani osób poniżej obowiązującego minimalnego wieku wymaganego do korzystania z usług internetowych bez zgody rodziców. W przypadku istotnych zmian w polityce zaktualizujemy powyższą datę i poinformujemy o nich w aplikacji lub na stronie internetowej.
