import { locale } from '../../i18n/index.svelte'

const rows = `
title|Credential use|凭据使用|Использование учётных данных|استخدام بيانات الاعتماد|Utilisation des identifiants|Uso de credenciales
local|Policies and history on this device only.|策略与记录仅保存在本设备。|Политики и история только на этом устройстве.|السياسات والسجل على هذا الجهاز فقط.|Politiques et historique sur cet appareil uniquement.|Políticas e historial solo en este dispositivo.
new|New policy|新建策略|Новая политика|سياسة جديدة|Nouvelle politique|Nueva política
name|Policy name|策略名称|Название политики|اسم السياسة|Nom de la politique|Nombre de política
owner|GitHub owner|GitHub 所有者|Владелец GitHub|مالك GitHub|Propriétaire GitHub|Propietario GitHub
repo|Repository|仓库|Репозиторий|المستودع|Dépôt|Repositorio
item|Vault item|秘密库条目|Элемент хранилища|عنصر الخزنة|Élément du coffre|Elemento de bóveda
field|Concealed credential field|隐藏凭据字段|Скрытое поле ключа|حقل اعتماد مخفي|Champ d’identifiant masqué|Campo de credencial oculto
choose|Choose…|请选择…|Выбрать…|اختر…|Choisir…|Elegir…
agent|Also grant this Agent permission to request uses|同时允许此 Agent 申请使用|Разрешить агенту запрашивать использование|السماح لهذا الوكيل بطلب الاستخدام|Autoriser cet agent à demander une utilisation|Permitir a este agente solicitar el uso
grantLifetime|Permission to request lasts until revoked; every use still needs a separate approval.|申请权限持续有效直至撤销，每次使用仍需另行批准。|Право запрашивать действует до отзыва; каждый раз нужно отдельное одобрение.|يستمر إذن الطلب حتى إلغائه؛ كل استخدام يحتاج موافقة منفصلة.|Le droit de demander dure jusqu’à révocation ; chaque utilisation exige une approbation.|El permiso para solicitar dura hasta su revocación; cada uso necesita otra aprobación.
none|No new Agent grant|不新增 Agent 授权|Без нового разрешения агенту|لا إذن جديد لوكيل|Aucune nouvelle autorisation d’agent|Sin nuevo permiso de agente
enabled|Enabled|已启用|Включено|مفعّل|Activé|Activado
disabled|Disabled|已禁用|Отключено|معطّل|Désactivé|Desactivado
disable|Disable now|立即禁用|Отключить сейчас|تعطيل الآن|Désactiver maintenant|Desactivar ahora
edit|Edit / enable|编辑 / 启用|Изменить / включить|تعديل / تفعيل|Modifier / activer|Editar / activar
delete|Request deletion|申请删除|Запросить удаление|طلب الحذف|Demander la suppression|Solicitar eliminación
submit|Review policy change|审查策略变更|Проверить изменение политики|مراجعة تغيير السياسة|Examiner la modification|Revisar cambio de política
cancel|Cancel|取消|Отмена|إلغاء|Annuler|Cancelar
empty|No policies on this device.|本设备尚无策略。|На устройстве нет политик.|لا سياسات على هذا الجهاز.|Aucune politique sur cet appareil.|No hay políticas en este dispositivo.
scope|Only create issues in this exact GitHub.com repository. Every use needs separate approval. A policy does not prove the upstream token has only these permissions.|仅在指定 GitHub.com 仓库创建 issue，每次使用另行审批。策略不证明上游 token 仅有这些权限。|Только создание задач в этом репозитории GitHub.com. Каждое использование одобряется отдельно. Политика не ограничивает права самого токена.|إنشاء مسائل في مستودع GitHub.com المحدد فقط. كل استخدام يحتاج موافقة منفصلة. لا تثبت السياسة صلاحيات الرمز الفعلية.|Création d’issues dans ce dépôt GitHub.com uniquement, avec approbation à chaque utilisation. La politique ne prouve pas les droits réels du jeton.|Solo crear issues en este repositorio GitHub.com, con aprobación en cada uso. La política no demuestra los permisos reales del token.
history|Use history|使用记录|История использования|سجل الاستخدام|Historique d’utilisation|Historial de uso
noHistory|No uses recorded on this device.|本设备尚无使用记录。|На устройстве нет записей.|لا استخدامات مسجلة على هذا الجهاز.|Aucune utilisation enregistrée ici.|No hay usos registrados aquí.
review|Review request|查看申请|Просмотреть запрос|مراجعة الطلب|Examiner la demande|Revisar solicitud
unknown|The issue may have been created. Check GitHub before creating a new request; dMsg will not resend this operation.|Issue 可能已创建。请先在 GitHub 核查再决定是否新建申请；dMsg 不会重发此操作。|Задача могла быть создана. Проверьте GitHub перед новым запросом; dMsg не повторит операцию.|ربما أُنشئت المسألة. تحقق من GitHub قبل طلب جديد؛ لن يعيد dMsg الإرسال.|L’issue a peut-être été créée. Vérifiez GitHub avant toute nouvelle demande ; dMsg ne renverra pas l’opération.|Es posible que el issue se haya creado. Compruebe GitHub antes de otra solicitud; dMsg no reenviará la operación.
mark|Mark manually checked|标记已人工核查|Отметить проверенным|وضع علامة تمت المراجعة يدويًا|Marquer comme vérifié|Marcar como revisado
reviewed|Manually checked; outcome remains unknown|已人工核查；结果状态仍为不明|Проверено вручную; исход неизвестен|تمت المراجعة؛ النتيجة ما زالت مجهولة|Vérifié manuellement ; résultat toujours inconnu|Revisado manualmente; resultado desconocido
more|Older uses|更早记录|Более ранние записи|استخدامات أقدم|Utilisations antérieures|Usos anteriores
effect|Approve one issue creation. Its title and body will be published to the target repository and may notify others.|批准创建一次 issue。下方标题和正文将发布到目标仓库，并可能通知他人。|Одобрение создаст одну задачу. Заголовок и текст будут опубликованы и могут вызвать уведомления.|توافق على إنشاء مسألة واحدة. سيُنشر العنوان والنص وقد تُرسل إشعارات.|Approuver la création d’une issue. Le titre et le texte seront publiés et pourront notifier d’autres personnes.|Aprobar la creación de un issue. Se publicarán título y texto, y podrían enviarse notificaciones.
reason|Agent-provided reason (untrusted)|Agent 自述理由（不可信输入）|Причина агента (недоверенный ввод)|سبب الوكيل (مدخل غير موثوق)|Motif fourni par l’agent (non fiable)|Motivo del agente (entrada no fiable)
body|Complete issue body|完整 issue 正文|Полный текст задачи|نص المسألة الكامل|Texte complet de l’issue|Cuerpo completo del issue
single|Single use; only issue number and link are returned. No credential value is delivered to the Agent.|仅执行一次，仅向 Agent 返回 issue 编号和链接，不交付凭据值。|Одно использование; агенту возвращаются только номер и ссылка, без ключа.|استخدام واحد؛ يُعاد للوكيل الرقم والرابط فقط، دون بيانات الاعتماد.|Utilisation unique ; seuls le numéro et le lien sont renvoyés, jamais l’identifiant secret.|Un solo uso; solo se devuelve número y enlace, nunca la credencial.
config|This changes a device-local policy. It does not approve an issue creation.|这会变更本设备策略，不代表批准创建 issue。|Изменяет локальную политику, но не одобряет создание задачи.|يغيّر سياسة محلية، ولا يوافق على إنشاء مسألة.|Modifie une politique locale, sans approuver la création d’une issue.|Cambia una política local, sin aprobar la creación de un issue.
reference|Reference (contains no secret value)|引用（不含秘密值）|Ссылка (без секретного значения)|مرجع (بلا قيمة سرية)|Référence (sans valeur secrète)|Referencia (sin valor secreto)
refresh|Refresh|刷新|Обновить|تحديث|Actualiser|Actualizar
version|Version|版本|Версия|الإصدار|Version|Versión
revokeHint|Disabling prevents new uses; an already sent request cannot be recalled.|禁用可阻止后续使用，已发送的请求无法撤回。|Отключение запрещает новые операции, но не отзывает отправленные.|يمنع التعطيل الاستخدامات الجديدة، ولا يسحب الطلبات المرسلة.|La désactivation bloque les nouveaux usages, sans rappeler les requêtes envoyées.|Desactivar impide nuevos usos, pero no retira solicitudes enviadas.
`.trim().split('\n').map((line) => line.split('|'))

const dictionary = Object.fromEntries(rows.map(([key, ...values]) => [key, values]))
export function secretText(key: string): string {
  const index = ['en', 'zh', 'ru', 'ar', 'fr', 'es'].indexOf(locale())
  return dictionary[key]?.[Math.max(0, index)] ?? key
}
