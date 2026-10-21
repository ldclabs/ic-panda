import { locale } from '../../i18n/index.svelte'

/** Wallet vocabulary kept together so forms and approval tickets use identical names. */
const rows = `
creationPaused|New team wallets are paused|已暂停新建团队钱包|Создание командных кошельков приостановлено|إنشاء محافظ الفرق متوقف مؤقتًا|Création de portefeuilles d’équipe suspendue|Creación de monederos de equipo pausada
creationPausedHint|Team wallets are being redesigned. Creating new wallets is temporarily unavailable.|团队钱包功能调整中，暂未开放新建。|Командные кошельки перерабатываются. Создание новых временно недоступно.|يجري إعادة تصميم محافظ الفرق. إنشاء محافظ جديدة غير متاح مؤقتًا.|Les portefeuilles d’équipe sont en cours de refonte. La création est temporairement indisponible.|Los monederos de equipo se están rediseñando. La creación no está disponible temporalmente.
cosignDisabled|Cosigning disabled|不要求协签|Совместная подпись отключена|التوقيع المشترك معطّل|Cosignature désactivée|Cofirma desactivada
softwareKey|Software key|软件密钥|Программный ключ|مفتاح برمجي|Clé logicielle|Clave de software
cosignRemove|Remove address cosigning|移除地址协签要求|Отключить подписи адреса|إزالة شرط التوقيع المشترك|Retirer la cosignature de l’adresse|Quitar cofirma de la dirección
cosignRemoveHint|Spending from this address will no longer require these signatures.|此地址的支出将不再要求这些协签。|Расходы с адреса больше не потребуют этих подписей.|لن يتطلب الإنفاق من هذا العنوان هذه التوقيعات.|Les dépenses de cette adresse ne nécessiteront plus ces signatures.|Los gastos de esta dirección ya no requerirán estas firmas.
cosignSetupHint|Create a local key, register it by proposal, then set the address policy after approval.|先创建本机密钥，再提交登记提案；批准生效后，才能设置地址协签。|Создайте локальный ключ и зарегистрируйте его предложением. После одобрения настройте адрес.|أنشئ مفتاحًا محليًا وسجّله بمقترح، ثم اضبط العنوان بعد الموافقة.|Créez une clé locale et enregistrez-la par proposition, puis configurez l’adresse après approbation.|Cree una clave local y regístrela mediante propuesta; configure la dirección tras la aprobación.
cosignSelect|Select 1–16 eligible cosigners and a valid signature threshold.|请选择 1–16 位合格协签人，并设置有效的协签阈值。|Выберите 1–16 подходящих ключей и допустимый порог.|اختر من 1 إلى 16 موقّعًا مؤهلًا وحدد حدًا صالحًا.|Choisissez 1 à 16 cosignataires admissibles et un seuil valide.|Seleccione de 1 a 16 cofirmantes aptos y un umbral válido.
cosignUnchanged|The address policy has not changed.|地址协签策略没有变化。|Политика адреса не изменилась.|لم تتغير سياسة العنوان.|La politique de l’adresse n’a pas changé.|La política de la dirección no ha cambiado.
cosignBound|Update these address policies before removing their cosigner or member.|请先修改以下地址的协签策略，再移除协签人或成员。|Сначала измените политики этих адресов, затем удалите ключ или участника.|عدّل سياسات هذه العناوين قبل إزالة الموقّع أو العضو.|Modifiez ces politiques d’adresse avant de retirer le cosignataire ou le membre.|Cambie estas políticas de dirección antes de quitar al cofirmante o miembro.
cosignHardwareUnreachable|The hardware keys cannot meet these address thresholds. Update the address policies first.|现有硬件密钥无法满足以下地址的阈值，请先调整地址协签策略。|Аппаратных ключей недостаточно. Сначала измените политики адресов.|لا تكفي المفاتيح العتادية لهذه الحدود. عدّل سياسات العناوين أولًا.|Les clés matérielles ne suffisent pas. Modifiez d’abord les politiques d’adresse.|Las claves físicas no alcanzan estos umbrales. Cambie primero las políticas de dirección.
preparePlan|Prepare signing plan|准备签名计划|Подготовить план подписи|إعداد خطة التوقيع|Préparer le plan de signature|Preparar plan de firma
renewPlan|Prepare a new plan|准备新计划|Подготовить новый план|إعداد خطة جديدة|Préparer un nouveau plan|Preparar un nuevo plan
cosignNoLocalKey|No eligible key on this device. Collect a packet from a cosigning device.|本设备没有合格的协签密钥，请从协签设备取得签名包。|На устройстве нет подходящего ключа. Получите пакет с другого устройства.|لا يوجد مفتاح مؤهل هنا. اجمع حزمة من جهاز موقّع.|Aucune clé admissible sur cet appareil. Obtenez un paquet d’un appareil signataire.|No hay una clave apta en este dispositivo. Obtenga un paquete de un dispositivo firmante.
collectOnlyAfter|This only collects your signatures. Transfer the packet and execute after all requirements are met.|本次只收集你的签名。传递签名包，满足全部协签要求后再单独执行。|Это только сбор подписей. Передайте пакет и выполните после достижения всех порогов.|يجمع هذا توقيعاتك فقط. انقل الحزمة ونفّذ بعد استيفاء جميع الشروط.|Cette action collecte seulement vos signatures. Transmettez le paquet, puis exécutez une fois tous les seuils atteints.|Esto solo recopila sus firmas. Transfiera el paquete y ejecute cuando se cumplan todos los requisitos.
cosignCallHint|A general call may spend from any ICP address; every policy below must be satisfied.|通用调用可能使用任意 ICP 地址的资产，必须满足以下全部协签策略。|Общий вызов может расходовать средства любого ICP-адреса; нужны все указанные политики.|قد ينفق الاستدعاء العام من أي عنوان ICP؛ يجب استيفاء جميع السياسات أدناه.|Un appel général peut dépenser depuis toute adresse ICP ; toutes les politiques ci-dessous sont requises.|Una llamada general puede gastar desde cualquier dirección ICP; deben cumplirse todas estas políticas.
votes|Votes recorded|投票记录|Записанные голоса|الأصوات المسجلة|Votes enregistrés|Votos registrados
nativeCosign|Native chain dual control|链上双控|Двойной контроль в сети|تحكم مزدوج على السلسلة|Double contrôle sur chaîne|Control doble en cadena
addAsset|Add asset|添加资产|Добавить актив|إضافة أصل|Ajouter un actif|Añadir activo
attached_cycles|Attached cycles|随调用转出的 cycles|Прикреплённые cycles|Cycles المرفقة|Cycles joints|Cycles adjuntos
refuel_rule|Automatic top-up rule|自动充值规则|Правило пополнения|قاعدة التعبئة التلقائية|Règle de recharge|Regla de recarga
kind|Result type|结果类型|Тип результата|نوع النتيجة|Type de résultat|Tipo de resultado
ledger|Ledger transfer|账本转账|Перевод в реестре|تحويل دفتر الأستاذ|Transfert de registre|Transferencia del libro
blocks|Ledger blocks|账本区块|Блоки реестра|كتل دفتر الأستاذ|Blocs du registre|Bloques del libro
block|Ledger block|账本区块|Блок реестра|كتلة دفتر الأستاذ|Bloc du registre|Bloque del libro
applied|Applied|已生效|Применено|تم التطبيق|Appliqué|Aplicado
chain_settled|Chain settlement|链上结算|Расчёт в сети|تسوية على السلسلة|Règlement sur chaîne|Liquidación en cadena
refueled|Cycles credited|Cycles 已充值|Cycles зачислены|تمت تعبئة Cycles|Cycles crédités|Cycles abonados
reply|Reply bytes|回复字节|Байты ответа|بايتات الرد|Octets de réponse|Bytes de respuesta
reply_text|Decoded reply|解码后的回复|Декодированный ответ|الرد المفكوك|Réponse décodée|Respuesta decodificada
reply_hash|Reply digest|回复摘要|Хеш ответа|بصمة الرد|Empreinte de réponse|Resumen de respuesta
call_unknown|Call outcome unknown|调用结果未知|Результат неизвестен|نتيجة الاستدعاء غير معروفة|Résultat inconnu|Resultado desconocido
arg|Argument bytes|参数字节|Байты аргументов|بايتات الوسائط|Octets des arguments|Bytes de argumentos
arg_text|Decoded arguments|解码后的参数|Декодированные аргументы|الوسائط المفكوكة|Arguments décodés|Argumentos decodificados
decimals|Decimal places|小数位数|Знаков после запятой|المنازل العشرية|Décimales|Decimales
accessHint|Access is unavailable. If you are still a member, join to bind this device; otherwise contact an administrator.|无法读取此钱包。如果你仍是成员，可加入以绑定本设备；否则请联系管理员。|Нет доступа. Если вы участник, привяжите устройство; иначе свяжитесь с администратором.|لا يتوفر الوصول. إن كنت عضوًا فاربط الجهاز، وإلا فاتصل بمسؤول.|Accès indisponible. Si vous êtes membre, associez cet appareil ; sinon contactez un administrateur.|Acceso no disponible. Si es miembro, vincule el dispositivo; si no, contacte al administrador.
overview|Overview|概览|Обзор|نظرة عامة|Vue d’ensemble|Resumen
proposals|Proposals|提案|Предложения|المقترحات|Propositions|Propuestas
assets|Assets & addresses|资产与地址|Активы и адреса|الأصول والعناوين|Actifs et adresses|Activos y direcciones
team|Members & policy|成员与策略|Участники и правила|الأعضاء والسياسة|Membres et règles|Miembros y política
activity|Audit|审计|Аудит|التدقيق|Audit|Auditoría
settings|Operations|运行管理|Управление|الإدارة|Gestion|Operaciones
newWallet|New wallet|新建钱包|Новый кошелёк|محفظة جديدة|Nouveau portefeuille|Nueva cartera
joinWallet|Join wallet|加入钱包|Вступить|الانضمام|Rejoindre|Unirse
activation_unknown|Activation status unavailable; retry from the approval request.|暂时无法读取激活状态，请从审批请求重试。|Статус недоступен; повторите из запроса.|حالة التفعيل غير متاحة؛ أعد المحاولة من الطلب.|État indisponible ; réessayez depuis la demande.|Estado no disponible; reintente desde la solicitud.
activation_unpaid|Preparing payment|准备付款|Подготовка оплаты|إعداد الدفع|Préparation du paiement|Preparando pago
activation_paid|Payment received|已收款|Оплата получена|تم استلام الدفع|Paiement reçu|Pago recibido
activation_creating|Creating wallet|正在创建钱包|Создание кошелька|إنشاء المحفظة|Création du portefeuille|Creando cartera
activation_installing|Installing wallet code|正在安装钱包代码|Установка кода|تثبيت شيفرة المحفظة|Installation du code|Instalando código
activation_ready|Wallet ready|钱包已就绪|Кошелёк готов|المحفظة جاهزة|Portefeuille prêt|Cartera lista
activation_failed|Creation failed — review the refund below|创建失败，请查看下方退款|Создание не удалось — проверьте возврат|فشل الإنشاء؛ راجع الاسترداد أدناه|Création échouée — voir le remboursement|Falló la creación; consulte el reembolso
activation_refunded|Refunded|已退款|Возвращено|تم الاسترداد|Remboursé|Reembolsado
activation_payment_unknown|Checking payment result|正在核对付款结果|Проверка оплаты|فحص نتيجة الدفع|Vérification du paiement|Comprobando pago
activation_payment_failed|Payment failed|付款失败|Ошибка оплаты|فشل الدفع|Paiement échoué|Pago fallido
loading|Loading…|正在读取…|Загрузка…|جارٍ التحميل…|Chargement…|Cargando…
retry|Retry|重试|Повторить|إعادة المحاولة|Réessayer|Reintentar
none|None|无|Нет|لا يوجد|Aucun|Ninguno
unknown|Unknown|未知|Неизвестно|غير معروف|Inconnu|Desconocido
all|All|全部|Все|الكل|Tout|Todo
waiting|My approvals|待我批准|Ждут меня|بانتظار موافقتي|À approuver|Por aprobar
executable|Ready to execute|待执行|К исполнению|جاهز للتنفيذ|À exécuter|Por ejecutar
more|Load more|加载更多|Загрузить ещё|تحميل المزيد|Afficher plus|Cargar más
findProposal|Find proposal ID|查找提案编号|Номер предложения|رقم المقترح|N° de proposition|ID de propuesta
details|Details|详情|Подробности|التفاصيل|Détails|Detalles
review|Review proposal|核对提案|Проверить|مراجعة المقترح|Vérifier|Revisar
submit|Submit proposal|提交提案|Предложить|إرسال المقترح|Proposer|Proponer
send|Transfer|转账|Перевод|تحويل|Transfert|Transferencia
receive|Receive|收款|Получить|استلام|Recevoir|Recibir
source|Data source|数据来源|Источник|مصدر البيانات|Source|Fuente
readAt|Read at|读取时间|Прочитано|وقت القراءة|Lu le|Lectura
stale|Refresh required|需要刷新|Обновите|يلزم التحديث|Actualiser|Actualizar
atomic|atomic units|最小单位|мин. единицы|وحدات أساسية|unités minimales|unidades mínimas
amount|Amount|金额|Сумма|المبلغ|Montant|Importe
fee|Network fee ceiling|网络手续费上限|Лимит комиссии|حد رسوم الشبكة|Plafond des frais|Límite de comisión
estimate|Estimate & check|估算并检查|Оценить и проверить|تقدير وفحص|Estimer et vérifier|Estimar y comprobar
estimated|Estimated fee|预计手续费|Оценка комиссии|الرسوم المقدرة|Frais estimés|Comisión estimada
reserve|Additional reserve / rent|额外储备／租金|Резерв / рента|احتياطي / إيجار|Réserve / loyer|Reserva / renta
previewHint|Estimate only. The wallet rechecks policy and fees when approving and executing.|仅为预估；钱包在批准与执行时重新核对策略和费用。|Оценка: кошелёк повторно проверяет правила и комиссии.|تقدير فقط؛ تعيد المحفظة فحص السياسة والرسوم.|Estimation : règles et frais sont revérifiés.|Estimación: la cartera vuelve a verificar reglas y comisiones.
recipient|Recipient|收款人|Получатель|المستلم|Destinataire|Destinatario
addRecipient|Add recipient|添加收款人|Добавить получателя|إضافة مستلم|Ajouter un destinataire|Añadir destinatario
remove|Remove|移除|Удалить|إزالة|Retirer|Quitar
memo|Memo (hex, optional)|Memo（十六进制，选填）|Memo (hex)|مذكرة (سداسي عشري)|Mémo (hex)|Memo (hex)
invalidAmount|Enter a positive amount with the asset’s supported precision.|请输入正数，且不要超过资产精度。|Введите положительную сумму допустимой точности.|أدخل مبلغًا موجبًا بدقة الأصل.|Saisissez un montant positif avec la précision admise.|Introduzca un importe positivo con la precisión admitida.
invalidId|Invalid account ID or Principal.|账户 ID 或 Principal 无效。|Неверный ID или Principal.|معرّف الحساب أو Principal غير صالح.|ID ou Principal invalide.|ID o Principal no válido.
nameBytes|Use at most 64 UTF-8 bytes.|最多 64 个 UTF-8 字节。|Не более 64 байт UTF-8.|64 بايت UTF-8 كحد أقصى.|64 octets UTF-8 maximum.|Máximo 64 bytes UTF-8.
noAssets|No spendable asset for this address. Review the wallet policy.|此地址没有可支出的资产，请查看钱包策略。|Нет разрешённых активов. Проверьте правила.|لا توجد أصول مسموحة لهذا العنوان.|Aucun actif autorisé. Consultez les règles.|Sin activos autorizados. Revise la política.
freeze|Freeze wallet|冻结钱包|Заморозить|تجميد المحفظة|Geler le portefeuille|Congelar cartera
freezeHint|Stops outgoing execution immediately. Unfreezing needs a proposal, admin approvals and the policy delay.|立即暂停对外执行。解冻需提案、管理员批准及策略规定的延时。|Исполнение остановится. Разморозка требует одобрений и задержки.|يوقف التنفيذ فورًا. يتطلب الإلغاء موافقات وفترة انتظار.|Arrêt immédiat. Le dégel exige des approbations et un délai.|Detiene la ejecución. Descongelar requiere aprobaciones y espera.
frozen|Frozen|已冻结|Заморожен|مجمّدة|Gelé|Congelada
unfreeze|Propose unfreezing|提议解冻|Предложить разморозку|اقتراح إلغاء التجميد|Proposer le dégel|Proponer descongelación
continue|Continue|继续|Продолжить|متابعة|Continuer|Continuar
reconcile|Check interrupted execution|核对中断执行|Проверить исполнение|فحص التنفيذ المنقطع|Vérifier l’exécution|Comprobar ejecución
rebroadcast|Broadcast again|重新广播|Повторить отправку|إعادة البث|Rediffuser|Volver a difundir
resign|Prepare replacement|准备替换交易|Заменить транзакцию|إعداد بديل|Préparer un remplacement|Preparar reemplazo
resignHint|Creates a new signing plan within the approved fee ceiling. Existing signatures for another plan cannot be reused.|在已批准的费用上限内建立新签名计划；其他计划的协签不可复用。|Новый план в пределах комиссии; старые подписи не подходят.|خطة جديدة ضمن حد الرسوم؛ لا يعاد استخدام توقيعات خطة أخرى.|Nouveau plan dans la limite approuvée ; signatures distinctes.|Nuevo plan dentro del límite aprobado; firmas independientes.
pending|Not yet confirmed on chain.|链上尚未确认。|Ещё не подтверждено.|لم يُؤكد على السلسلة بعد.|Pas encore confirmé.|Aún sin confirmar.
voted|You already voted|你已投票|Вы уже голосовали|لقد صوّت بالفعل|Vous avez déjà voté|Ya ha votado
before|Before|变更前|До|قبل|Avant|Antes
after|After|变更后|После|بعد|Après|Después
unsupported|Unsupported proposal. Approval is disabled.|无法完整解析此提案，已禁用批准。|Предложение не поддерживается.|المقترح غير مدعوم؛ الموافقة معطلة.|Proposition non prise en charge.|Propuesta no compatible; aprobación desactivada.
policy|Policy|策略|Правила|السياسة|Règles|Política
editPolicy|Change policy|修改策略|Изменить правила|تعديل السياسة|Modifier les règles|Cambiar política
addMember|Invite member|邀请成员|Пригласить|دعوة عضو|Inviter un membre|Invitar miembro
editMember|Change member|修改成员|Изменить участника|تعديل العضو|Modifier le membre|Cambiar miembro
id|ID|标识|ID|المعرّف|ID|ID
role|Role|角色|Роль|الدور|Rôle|Rol
label|Label|标签|Метка|التسمية|Libellé|Etiqueta
agent_ids|Agent IDs|Agent ID 列表|ID агентов|معرّفات الوكلاء|ID des agents|ID de agentes
principals|Bound device principals|已绑定设备身份|Привязанные устройства|هويات الأجهزة|Identités des appareils|Identidades de dispositivos
invite|Copy invitation|复制邀请|Копировать приглашение|نسخ الدعوة|Copier l’invitation|Copiar invitación
recoveryHint|Recovery members may propose membership changes only. A seven-day delay gives admins and approvers time to veto.|恢复成员仅能提议成员变更；七天延时内管理员和审批人可否决。|Восстановление меняет участников с задержкой семь дней и правом вето.|التعافي يغير العضوية فقط مع انتظار سبعة أيام وحق النقض.|Le rétablissement modifie les membres avec sept jours de veto.|Recuperación cambia miembros con siete días de veto.
book|Address book|地址簿|Адресная книга|دفتر العناوين|Carnet d’adresses|Libreta de direcciones
addBook|Add recipient|新增地址簿条目|Добавить адрес|إضافة عنوان|Ajouter une adresse|Añadir dirección
cooldown|Available after|冷却结束时间|Доступен после|متاح بعد|Disponible après|Disponible después
addAddress|New address|新增地址|Новый адрес|عنوان جديد|Nouvelle adresse|Nueva dirección
chain|Network|网络|Сеть|الشبكة|Réseau|Red
control|Address control|地址控制方式|Контроль адреса|التحكم بالعنوان|Contrôle de l’adresse|Control de dirección
cosigners|Wallet cosigners|钱包协签人|Соподписанты|الموقّعون المشاركون|Cosignataires|Cofirmantes
localKeys|Keys on this device|本设备密钥|Ключи устройства|مفاتيح هذا الجهاز|Clés de cet appareil|Claves del dispositivo
hardwareOnly|Require a hardware key|必须使用硬件密钥|Только аппаратный ключ|مفتاح عتادي إلزامي|Clé matérielle requise|Exigir clave física
softwareWarning|Software keys can be copied. They do not provide hardware isolation.|软件密钥可被复制，不具备硬件隔离。|Программные ключи можно скопировать.|يمكن نسخ المفاتيح البرمجية؛ لا عزل عتادي.|Les clés logicielles peuvent être copiées.|Las claves de software pueden copiarse.
register|Register key|登记密钥|Зарегистрировать|تسجيل المفتاح|Enregistrer la clé|Registrar clave
bindCosign|Set address cosigners|设置地址协签人|Назначить подписи|تعيين موقّعين للعنوان|Définir les cosignataires|Definir cofirmantes
threshold|Required signatures|所需协签数|Нужно подписей|التوقيعات المطلوبة|Signatures requises|Firmas requeridas
cosignPlan|Cosign this plan|协签此计划|Подписать план|توقيع هذه الخطة|Cosigner ce plan|Cofirmar este plan
packet|Cosignature packet|协签包|Пакет подписей|حزمة توقيعات|Paquet de signatures|Paquete de firmas
importPacket|Import signatures|导入协签|Импорт подписей|استيراد التوقيعات|Importer les signatures|Importar firmas
copyPacket|Copy signatures|复制协签包|Копировать подписи|نسخ التوقيعات|Copier les signatures|Copiar firmas
collected|Collected / required|已收集／所需|Собрано / нужно|المجمّع / المطلوب|Reçues / requises|Recibidas / requeridas
collectHint|Share packets between cosigning devices. Import only collects signatures; execution is a separate action.|在协签设备间传递协签包。导入仅收集签名，执行需单独操作。|Импорт только собирает подписи; исполнение отдельно.|الاستيراد يجمع التوقيعات فقط؛ التنفيذ منفصل.|L’import collecte les signatures ; l’exécution est séparée.|Importar recopila firmas; ejecutar es una acción aparte.
executeCollected|Execute with signatures|使用已收集协签执行|Исполнить с подписями|تنفيذ بالتوقيعات|Exécuter avec les signatures|Ejecutar con firmas
plan|Execution plan|执行计划|План исполнения|خطة التنفيذ|Plan d’exécution|Plan de ejecución
cycles|Cycles reserve|Cycles 储备|Резерв cycles|احتياطي Cycles|Réserve de cycles|Reserva de cycles
refuel|Propose a top-up|提议充值|Предложить пополнение|اقتراح تعبئة|Proposer une recharge|Proponer recarga
autoRefuel|Run configured top-up|按规则触发充值|Пополнить по правилам|تعبئة وفق السياسة|Recharger selon les règles|Recargar según reglas
lowCycles|Cycles below the configured threshold.|Cycles 低于设定水位。|Cycles ниже порога.|Cycles أقل من الحد.|Cycles sous le seuil.|Cycles por debajo del umbral.
health|Read canister health|读取运行状况|Состояние канистры|حالة الحاوية|État du canister|Estado del canister
idleDays|Days at current idle burn (excludes signing)|按当前空闲消耗估算天数（不含签名）|Дни при простое, без подписей|أيام الخمول دون توقيع|Jours au repos, hors signatures|Días en reposo, sin firmas
versions|Code versions|代码版本|Версии кода|إصدارات الشيفرة|Versions du code|Versiones del código
upgrade|Propose an upgrade|提议升级|Предложить обновление|اقتراح ترقية|Proposer une mise à jour|Proponer actualización
applyUpgrade|Install accepted version|安装已接受版本|Установить принятую версию|تثبيت الإصدار المعتمد|Installer la version acceptée|Instalar versión aceptada
lockCode|Propose permanent code lock|提议永久锁定代码|Заблокировать код навсегда|اقتراح قفل دائم للشيفرة|Proposer le verrouillage définitif|Proponer bloqueo permanente
lockHint|After controllers are removed, nobody can upgrade or repair this canister. This is irreversible.|移除控制者后，任何人都无法升级或修复此 canister，且不可撤销。|После удаления контроллеров обновление невозможно. Необратимо.|إزالة المتحكمين تمنع الترقية والإصلاح نهائيًا.|Sans contrôleurs, aucune mise à jour ou réparation. Irréversible.|Sin controladores no se puede actualizar ni reparar. Irreversible.
applyLock|Apply accepted code lock|执行已接受的代码锁定|Применить блокировку|تطبيق القفل المعتمد|Appliquer le verrouillage|Aplicar bloqueo aceptado
rpc|Chain connections|链连接|Подключения к сетям|اتصالات السلاسل|Connexions aux chaînes|Conexiones de red
rpcHint|Saved for this account on this device. HTTPS endpoints only outside local deployments.|仅保存在本机当前账户；非本地部署只允许 HTTPS。|Сохраняется локально для аккаунта. Только HTTPS.|يحفظ محليًا للحساب؛ HTTPS فقط خارج البيئة المحلية.|Enregistré localement pour ce compte. HTTPS uniquement.|Guardado localmente para esta cuenta. Solo HTTPS.
checkConnections|Check connections|检查连接|Проверить подключения|فحص الاتصالات|Tester les connexions|Comprobar conexiones
save|Save|保存|Сохранить|حفظ|Enregistrer|Guardar
saved|Saved|已保存|Сохранено|تم الحفظ|Enregistré|Guardado
channel|Notification channel|通知频道|Канал уведомлений|قناة الإشعارات|Canal de notifications|Canal de notificaciones
channelHint|Messages are encrypted hints. Open the wallet to verify current state.|消息是加密提示；打开钱包核对当前状态。|Сообщения — подсказки. Состояние проверяйте в кошельке.|الرسائل تلميحات مشفرة؛ تحقق من الحالة بالمحفظة.|Messages chiffrés indicatifs ; vérifiez dans le portefeuille.|Avisos cifrados; verifique el estado en la cartera.
notify|Publish current state|发布当前状态|Опубликовать состояние|نشر الحالة|Publier l’état|Publicar estado
notificationFailed|Wallet updated; channel notification failed. Retry from Operations.|钱包已更新，频道通知失败；可在运行管理中重试。|Кошелёк обновлён; уведомление не отправлено.|تم تحديث المحفظة وتعذر إرسال الإشعار.|Portefeuille mis à jour ; notification échouée.|Cartera actualizada; falló la notificación.
export|Export this verified page|导出本页核验记录|Экспорт страницы|تصدير الصفحة المتحققة|Exporter cette page vérifiée|Exportar página verificada
verifiedHint|The audit hash chain matches the certified wallet head. This does not prove external-chain settlement.|审计哈希链与认证的钱包链头一致；这不证明外链交易已经结算。|Цепочка совпадает с заверенным хешем, но не доказывает расчёт во внешней сети.|تطابق التدقيق لا يثبت تسوية السلسلة الخارجية.|La chaîne correspond à la tête certifiée, sans prouver le règlement externe.|La cadena coincide con la cabecera certificada; no prueba liquidación externa.
activeHint|Counts reflect loaded proposals. Load older pages to inspect more.|数量基于已加载的提案；更早记录可继续加载。|Счётчик учитывает загруженные предложения.|العدد يشمل المقترحات المحملة فقط.|Le compteur concerne les propositions chargées.|El contador refleja las propuestas cargadas.
openApproval|Open approval request|打开审批请求|Открыть запрос|فتح طلب الموافقة|Ouvrir la demande|Abrir solicitud
payer|Paying identity|付款身份|Плательщик|هوية الدفع|Identité payeuse|Identidad pagadora
readiness|Check approval readiness|检查批准资格|Проверить готовность|فحص أهلية الموافقة|Vérifier l’autorisation|Comprobar autorización
WALLET_ACCOUNT_FROZEN|Account approvals are frozen.|账户批准已冻结。|Одобрения аккаунта заморожены.|موافقات الحساب مجمدة.|Les approbations du compte sont gelées.|Las aprobaciones están congeladas.
WALLET_APPROVE_REQUIRED|Enable WalletApprove for this device in Settings → Devices.|请在设置 → 设备中为本设备开启 WalletApprove。|Включите WalletApprove в настройках устройства.|فعّل WalletApprove في إعدادات الأجهزة.|Activez WalletApprove dans les paramètres des appareils.|Active WalletApprove en los ajustes de dispositivos.
WALLET_REGISTRATION_REQUIRED|Wallet service registration is missing.|钱包服务尚未登记。|Сервис кошелька не зарегистрирован.|خدمة المحفظة غير مسجلة.|Service de portefeuille non enregistré.|Servicio de cartera no registrado.
WALLET_REGISTRATION_UNAVAILABLE|Cannot verify the wallet service. Retry before paying.|无法核对钱包服务，请重试后再付款。|Не удалось проверить сервис. Повторите до оплаты.|تعذر التحقق من الخدمة؛ أعد المحاولة قبل الدفع.|Vérification impossible. Réessayez avant de payer.|No se pudo verificar. Reintente antes de pagar.
FEE_TOO_LOW|Fee ceiling is below the estimate.|手续费上限低于估算值。|Лимит ниже оценки.|حد الرسوم أقل من التقدير.|Plafond inférieur à l’estimation.|Límite inferior a la estimación.
INSUFFICIENT_NATIVE_BALANCE|Insufficient balance for amount, fees and reserve.|余额不足以支付金额、费用与储备。|Не хватает средств с учётом комиссии.|الرصيد لا يغطي المبلغ والرسوم والاحتياطي.|Solde insuffisant avec frais et réserve.|Saldo insuficiente con comisiones y reserva.
INSUFFICIENT_ASSET_BALANCE|Insufficient token balance.|代币余额不足。|Недостаточно токенов.|رصيد الرموز غير كافٍ.|Solde du jeton insuffisant.|Saldo del token insuficiente.
Admin|Administrator|管理员|Администратор|مسؤول|Administrateur|Administrador
Approver|Approver|审批人|Утверждающий|موافق|Approbateur|Aprobador
Proposer|Proposer|提案人|Инициатор|مقترِح|Proposant|Proponente
Viewer|Viewer|只读成员|Наблюдатель|مشاهد|Lecteur|Lector
Recovery|Recovery member|恢复成员|Восстановление|عضو تعافٍ|Rétablissement|Recuperación
Active|Active|活跃|Активен|نشط|Actif|Activo
Invited|Invited|受邀|Приглашён|مدعو|Invité|Invitado
Open|Awaiting votes|等待投票|Ждёт голосов|بانتظار التصويت|En attente de votes|Esperando votos
Delayed|Delay in progress|延时中|Задержка|فترة انتظار|Délai en cours|En espera
Ready|Ready|可执行|Готово|جاهز|Prêt|Lista
Executing|Executing|执行中|Исполняется|قيد التنفيذ|En cours|Ejecutándose
Signed|Signed|已签名|Подписано|موقّع|Signé|Firmada
Broadcast|Broadcast|已广播|Отправлено|تم البث|Diffusé|Difundida
Settled|Completed|已完成|Завершено|مكتمل|Terminé|Completada
Failed|Failed|失败|Ошибка|فشل|Échec|Fallida
Rejected|Rejected|已拒绝|Отклонено|مرفوض|Refusé|Rechazada
Vetoed|Vetoed|已否决|Вето|منقوض|Veto|Vetada
Cancelled|Cancelled|已取消|Отменено|ملغى|Annulé|Cancelada
Expired|Expired|已过期|Истекло|منتهي|Expiré|Caducada
Stale|Policy changed|策略已变更|Правила изменены|تغيرت السياسة|Règles modifiées|Política modificada
true|Yes|是|Да|نعم|Oui|Sí
false|No|否|Нет|لا|Non|No
transfer|Transfer|转账|Перевод|تحويل|Transfert|Transferencia
batch|Batch transfer|批量转账|Пакетный перевод|تحويل جماعي|Transfert groupé|Transferencia múltiple
canister_call|Canister call|Canister 调用|Вызов канистры|استدعاء حاوية|Appel de canister|Llamada a canister
add_address|Add address|新增地址|Добавить адрес|إضافة عنوان|Ajouter une adresse|Añadir dirección
set_cosign|Change cosigning|修改协签策略|Изменить подписи|تعديل التوقيع المشترك|Modifier la cosignature|Cambiar cofirmas
add_cosigner|Register cosigner|登记协签人|Добавить соподписанта|تسجيل موقّع|Ajouter un cosignataire|Registrar cofirmante
remove_cosigner|Remove cosigner|移除协签人|Удалить соподписанта|إزالة موقّع|Retirer un cosignataire|Quitar cofirmante
add_member|Invite member|邀请成员|Пригласить участника|دعوة عضو|Inviter un membre|Invitar miembro
update_member|Change member|修改成员|Изменить участника|تعديل عضو|Modifier un membre|Cambiar miembro
remove_member|Remove member|移除成员|Удалить участника|إزالة عضو|Retirer un membre|Quitar miembro
set_policy|Change policy|修改策略|Изменить правила|تعديل السياسة|Modifier les règles|Cambiar política
book_add|Add recipient|新增收款地址|Добавить получателя|إضافة مستلم|Ajouter un destinataire|Añadir destinatario
book_remove|Remove recipient|移除收款地址|Удалить получателя|إزالة مستلم|Retirer un destinataire|Quitar destinatario
accept_upgrade|Accept code version|接受代码版本|Принять версию|اعتماد الإصدار|Accepter une version|Aceptar versión
lock_code|Lock code permanently|永久锁定代码|Блокировка кода|قفل دائم للشيفرة|Verrouiller définitivement|Bloquear código permanentemente
cancel_evm_nonce|Cancel pending EVM transaction|取消待确认 EVM 交易|Отменить EVM-транзакцию|إلغاء معاملة EVM معلقة|Annuler la transaction EVM|Cancelar transacción EVM
release_evm_nonce|Release EVM nonce|释放 EVM nonce|Освободить nonce|تحرير nonce|Libérer le nonce|Liberar nonce
init_sol_nonce|Initialize durable nonce|初始化持久 nonce|Создать durable nonce|تهيئة nonce دائم|Initialiser le nonce durable|Inicializar nonce duradero
from|Source address|来源地址|Исходный адрес|عنوان المصدر|Adresse source|Dirección de origen
to|Recipient|收款地址|Получатель|المستلم|Destinataire|Destinatario
asset|Asset|资产|Актив|الأصل|Actif|Activo
outputs|Recipients|收款列表|Получатели|المستلمون|Destinataires|Destinatarios
max_fee|Fee ceiling|手续费上限|Лимит комиссии|حد الرسوم|Plafond des frais|Límite de comisión
member|Member|成员|Участник|العضو|Membre|Miembro
address|Address ID|地址编号|Номер адреса|رقم العنوان|N° d’adresse|ID de dirección
cosign|Cosigning|协签|Совместная подпись|توقيع مشترك|Cosignature|Cofirma
public_key|Public key|公钥|Публичный ключ|المفتاح العام|Clé publique|Clave pública
hardware|Hardware key|硬件密钥|Аппаратный ключ|مفتاح عتادي|Clé matérielle|Clave física
version|Version|版本|Версия|الإصدار|Version|Versión
module_hash|Module SHA-256|模块 SHA-256|SHA-256 модуля|SHA-256 للوحدة|SHA-256 du module|SHA-256 del módulo
approver_threshold|Spending approvals|支出批准数|Подписи расходов|موافقات الإنفاق|Approbations des dépenses|Aprobaciones de gastos
admin_threshold|Admin approvals|管理批准数|Подписи админов|موافقات المسؤولين|Approbations des administrateurs|Aprobaciones administrativas
tiers|Spending tiers|支出分档|Уровни расходов|شرائح الإنفاق|Paliers de dépenses|Tramos de gasto
up_to|Upper amount (atomic)|金额上限（最小单位）|Верхняя сумма|الحد الأعلى|Montant maximal|Importe máximo
approvals|Required approvals|所需批准数|Нужно одобрений|الموافقات المطلوبة|Approbations requises|Aprobaciones requeridas
delay_ms|Delay (ms)|延时（毫秒）|Задержка (мс)|تأخير (مللي ثانية)|Délai (ms)|Espera (ms)
book_only|Address book only|仅限地址簿|Только адресная книга|دفتر العناوين فقط|Carnet uniquement|Solo libreta
address_book_cooldown_ms|Address cooldown (ms)|地址簿冷却（毫秒）|Задержка адреса (мс)|انتظار العنوان|Délai des adresses (ms)|Espera de direcciones (ms)
daily_limits|24-hour limits|24 小时限额|Суточные лимиты|حدود 24 ساعة|Limites sur 24 h|Límites de 24 h
spendable_assets|Spendable assets|可支出资产|Разрешённые активы|الأصول المسموحة|Actifs autorisés|Activos permitidos
call_targets|Allowed canisters|允许调用的 canister|Разрешённые канистры|الحاويات المسموحة|Canisters autorisés|Canisters permitidos
evm_chains|EVM chain IDs|EVM 链 ID|ID сетей EVM|معرّفات سلاسل EVM|ID des chaînes EVM|ID de redes EVM
proposal_ttl_ms|Proposal lifetime (ms)|提案有效期（毫秒）|Срок предложения (мс)|مدة المقترح|Durée du vote (ms)|Vigencia de propuesta (ms)
unfreeze_delay_ms|Unfreeze delay (ms)|解冻延时（毫秒）|Задержка разморозки|انتظار إلغاء التجميد|Délai de dégel (ms)|Espera de descongelación (ms)
daily_signatures|Daily signature budget|每日签名预算|Подписей в день|ميزانية التوقيعات اليومية|Signatures par jour|Firmas diarias
hardware_cosigners|Require hardware cosigners|协签必须使用硬件|Только аппаратные подписи|موقّعون عتاديون فقط|Cosignataires matériels requis|Exigir cofirmantes físicos
public|Publicly readable|允许公开读取|Публичное чтение|قراءة عامة|Lecture publique|Lectura pública
min_cycles|Low-water mark (cycles)|低水位（cycles）|Минимум cycles|حد Cycles الأدنى|Seuil de cycles|Umbral de cycles
icp_e8s|ICP amount (e8s)|ICP 金额（e8s）|ICP (e8s)|مبلغ ICP (e8s)|Montant ICP (e8s)|Importe ICP (e8s)
max_per_day|Top-ups per day|每日充值次数|Пополнений в день|التعبئات اليومية|Recharges par jour|Recargas diarias
chain_id|Chain ID|链 ID|ID сети|معرّف السلسلة|ID de chaîne|ID de red
consent|Canister consent text|Canister 同意文本|Текст согласия|نص الموافقة|Texte de consentement|Texto de consentimiento
`
const vocabulary = Object.fromEntries(
  rows
    .trim()
    .split('\n')
    .map((row) => {
      const [key, ...values] = row.split('|')
      return [key, values]
    })
)
const columns: Record<string, number> = { en: 0, zh: 1, ru: 2, ar: 3, fr: 4, es: 5 }
export function w(key: string): string {
  return vocabulary[key]?.[columns[locale()]] ?? vocabulary[key]?.[0] ?? key
}
