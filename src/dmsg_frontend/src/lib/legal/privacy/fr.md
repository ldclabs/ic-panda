# Politique de confidentialité de dMsg

Dernière mise à jour : 2026-10-11.

dMsg est conçu pour que votre contenu privé soit chiffré sur vos propres appareils. Cette politique explique quelles informations traitent le site dMsg, l’ancienne application web, l’extension de navigateur, le service cloud et les canisters sur Internet Computer (ICP), qui peut les voir et quels choix s’offrent à vous.

Nous ne vendons pas de données personnelles, nous n’utilisons pas votre contenu à des fins publicitaires, et le site ne contient aucun traceur d’analyse ou de publicité.

## 1. Champ d’application

Cette politique couvre dmsg.net, l’ancienne application web en lecture seule, l’extension de navigateur dMsg, le service cloud dMsg et les canisters dMsg. « Nous » désigne l’équipe qui exploite dMsg sous la gouvernance d’ICPanda DAO. Les portefeuilles, Internet Identity, les registres de jetons et les applications que vous connectez ont leurs propres pratiques de confidentialité.

## 2. Contenu privé

- Les notes, messages, fichiers et éléments du coffre sont chiffrés sur vos appareils approuvés et déverrouillés avant d’être envoyés. Le cloud ne stocke et ne distribue que des données chiffrées.
- Les clés de contenu restent sur vos appareils, enveloppées pour chaque appareil que vous approuvez. Pour la récupération, le compte conserve aussi une enveloppe qui ne peut être ouverte que par le service vetKD d’ICP, selon les règles de récupération du compte.
- Nous ne recevons ni votre contenu en clair ni vos clés de contenu.
- Les personnes avec qui vous partagez du contenu peuvent le lire et en garder des copies.

## 3. Informations non chiffrées

Le chiffrement protège le contenu, pas chaque relation ou action. Pour faire fonctionner le service, le service cloud et les canisters traitent :

- les identifiants de compte, les principals de connexion, les clés publiques et identifiants des appareils ;
- les noms, les profils publics et les règles de contact que vous définissez ;
- l’appartenance aux canaux et les relations de contact nécessaires pour appliquer vos règles ;
- la taille et l’horodatage des objets, messages et requêtes ;
- les adresses IP et les informations des requêtes qui parviennent au service cloud ;
- les enregistrements de commandes, de paiements, de remboursements et de droits.

## 4. Informations publiques et on-chain

Certaines informations sont publiques par conception :

- les noms et les comptes qui les détiennent ;
- les documents d’annuaire servis sur id.dmsg.net, y compris leurs contrôleurs, leur service de délégation et le lien de profil ;
- les informations de profil que vous choisissez de publier ;
- les transactions sur les registres de jetons ICP, y compris les paiements à dMsg et les remboursements de dMsg.

L’état du compte que d’autres services doivent vérifier, comme les droits liés à la formule et les reçus des signatures formelles, est publié sous forme de données certifiées. Les enregistrements écrits sur ICP ne peuvent généralement pas être effacés.

## 5. Paiements et adhésion PANDA

Vous payez depuis votre propre portefeuille sur des registres ICP comme ckUSDC et PANDA. Les canisters commerce, payment et handle de dMsg conservent les enregistrements des commandes, séquestres, dépôts, remboursements et reçus, y compris le compte payeur et les montants. Si vous demandez une exonération PANDA, le canister membership lit dans le SNS PANDA la mise, l’état de dissolution et les principals de votre neurone.

## 6. Données sur votre appareil

- L’extension conserve dans le navigateur vos clés, une base de données locale chiffrée, les modifications non envoyées, l’état de synchronisation et les réglages.
- Le site enregistre la langue choisie dans le stockage local.
- L’ancienne application conserve votre session et des données en cache dans l’IndexedDB du navigateur, et son service worker met en cache les fichiers du site. Sa vue portefeuille demande les prix des jetons à l’API ICPSwap.

Désinstaller l’extension ou effacer les données du site supprime ces copies locales. Le contenu non synchronisé avec le cloud est alors perdu.

## 7. Prestataires

Nous faisons appel aux prestataires suivants uniquement dans la mesure nécessaire au fonctionnement de dMsg :

- Cloudflare exécute le service cloud et traite les données chiffrées et les informations des requêtes qui lui sont envoyées.
- Les fournisseurs de nœuds ICP exécutent les canisters, et des passerelles HTTP servent dmsg.net et id.dmsg.net. Ils peuvent conserver des journaux de requêtes standard.
- Internet Identity et les portefeuilles que vous choisissez gèrent votre connexion et vos paiements.
- Le Chrome Web Store distribue l’extension.

Nous pouvons divulguer des informations lorsque la loi l’exige. Le contenu étant chiffré, nous ne pouvons divulguer que les données chiffrées et les métadonnées que nous détenons.

## 8. Conservation

- Les données chiffrées restent dans le cloud tant que votre compte les utilise et dans la mesure permise par votre formule. Lorsque vous supprimez un élément, ses données chiffrées sont retirées selon les règles affichées dans l’extension.
- À la fin d’une formule payante, les Conditions d’utilisation décrivent le sort du contenu au-delà des limites gratuites.
- Les journaux d’exploitation ne sont conservés que le temps nécessaire à la sécurité, à la prévention des abus et au dépannage.
- Les noms, transactions de registre, reçus de signature et autres enregistrements écrits sur ICP sont conservés par les canisters et les registres et ne peuvent généralement pas être supprimés.

## 9. Vos choix et vos droits

Dans l’extension, vous pouvez exporter vos données, supprimer des éléments, révoquer des appareils et modifier ou fermer vos règles de contact. Selon votre lieu de résidence, vous pouvez avoir le droit d’accéder à vos informations personnelles, de les rectifier ou de les supprimer, ou de vous opposer à leur utilisation. Contactez-nous via la page d’assistance. Comme nous ne pouvons ni déchiffrer votre contenu ni effacer les enregistrements on-chain, certaines demandes ne peuvent être satisfaites qu’en partie.

## 10. Sécurité et ses limites

dMsg utilise le chiffrement sur l’appareil, des approbations distinctes pour la connexion, les appareils et les signatures, et des données certifiées que les clients peuvent vérifier. Aucun système n’est parfaitement sûr. Les canisters dMsg sont contrôlés par la gouvernance d’ICPanda DAO, qui peut les mettre à niveau et modifier leurs règles, et les mises à jour de l’extension constituent un autre point de confiance. Signalez les vulnérabilités en privé via la page d’assistance.

## 11. Enfants

dMsg ne s’adresse pas aux enfants de moins de 13 ans, ni à ceux qui n’ont pas l’âge minimum requis dans leur pays pour utiliser des services en ligne sans consentement parental.

## 12. Modifications et contact

Lorsque cette politique change, nous mettons à jour la date ci-dessus et annonçons les changements importants sur le site ou dans l’extension. Pour toute question ou demande, consultez l’[Assistance](/_/support).
