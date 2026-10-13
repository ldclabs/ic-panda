/**
 * Text of the tray, notifications, update prompts and file dialogs, in the
 * interface language the main process resolved (`shared/locales.ts`); the
 * windows show theirs from the same choice (`renderer/i18n`).
 */
import { defaultLocale, matchLocale, type Locale } from '../shared/locales.ts'

const enStates: Record<string, string> = {
  connecting: 'connecting',
  unknown: 'unknown',
  logged_out: 'logged out',
  pairing_required: 'waiting to pair',
  no_account: 'no account yet',
  locked: 'locked',
  root_required: 'content root needed',
  unlocked: 'unlocked'
}

const en = {
  state: (state: string) => enStates[state] ?? state,
  waiting: (n: number) => `${n} request(s) waiting`,
  tooltip: (version: string, state: string, n: number) => `dMsg ${version} — ${state}${n ? `, ${n} waiting` : ''}`,
  open: 'Open dMsg',
  lock: 'Lock',
  openAtLogin: 'Open at Login',
  quit: 'Quit',
  checkUpdates: 'Check for Updates',
  checking: 'Checking for Updates…',
  downloading: 'Downloading Update…',
  restartToUpdate: (version: string) => `Restart to Update to ${version}`,
  requestWaiting: 'A request is waiting for your approval.',
  eventsArrived: (n: number) => `Your agents received ${n} new event(s).`,
  refreshDue: "An agent's event subscription is due for renewal.",
  updateLater: 'dMsg will update when its work in flight settles',
  updateBusy: (busy: string) => `Still in flight: ${busy}. Unlock dMsg so it can finish, then choose Restart to Update again.`,
  inFlightApprovals: (n: number) => `${n} approved request(s) whose outcome is not settled`,
  inFlightAccount: 'an account update whose outcome is not known',
  inFlightRoot: 'a root rotation',
  separator: ', ',
  allFiles: 'All files',
  migrationFile: 'dMsg migration',
  frozenFile: 'Frozen history',
  bundleFile: 'Attestation bundle',
  pickBundle: 'Choose the bundle to verify',
  pickOriginal: 'Choose the original file of the bundle',
  pickAction: 'Choose the action the application prepared (CBOR)',
  pickAvatar: 'Choose a public avatar (PNG, JPEG or WebP, at most 512 KiB)',
  imageFile: 'Images',
  newMessages: 'New channel messages in dMsg.',
  newLetter: 'A new letter arrived in dMsg.'
}

type Text = typeof en

const zhStates: Record<string, string> = {
  connecting: '连接中',
  unknown: '未知',
  logged_out: '未登录',
  pairing_required: '等待配对',
  no_account: '尚未创建账户',
  locked: '已锁定',
  root_required: '需要内容根',
  unlocked: '已解锁'
}

const zh: Text = {
  state: (state: string) => zhStates[state] ?? state,
  waiting: (n: number) => `${n} 个请求待确认`,
  tooltip: (version: string, state: string, n: number) => `dMsg ${version} — ${state}${n ? `，${n} 个待确认` : ''}`,
  open: '打开 dMsg',
  lock: '锁定',
  openAtLogin: '登录时打开',
  quit: '退出',
  checkUpdates: '检查更新',
  checking: '正在检查更新…',
  downloading: '正在下载更新…',
  restartToUpdate: (version: string) => `重启并更新到 ${version}`,
  requestWaiting: '有请求等待你确认。',
  eventsArrived: (n: number) => `Agent 收到 ${n} 条新事件。`,
  refreshDue: '有 Agent 的事件订阅需要续订。',
  updateLater: 'dMsg 会在进行中的操作完成后更新',
  updateBusy: (busy: string) => `仍在进行：${busy}。解锁 dMsg 让它完成，然后再选“重启并更新”。`,
  inFlightApprovals: (n: number) => `${n} 个已批准、结果尚未确定的请求`,
  inFlightAccount: '一次结果未知的账户更新',
  inFlightRoot: '一次换根',
  separator: '、',
  allFiles: '所有文件',
  migrationFile: 'dMsg 迁移文件',
  frozenFile: '冻结历史',
  bundleFile: '证据包',
  pickBundle: '选择要核验的证据包',
  pickOriginal: '选择证据包对应的原文件',
  pickAction: '选择应用准备的动作（CBOR）',
  pickAvatar: '选择公开头像（PNG、JPEG 或 WebP，最大 512 KiB）',
  imageFile: '图片',
  newMessages: 'dMsg 有新的频道消息。',
  newLetter: 'dMsg 有新来信。'
}

const ruStates: Record<string, string> = {
  connecting: 'подключение',
  unknown: 'неизвестно',
  logged_out: 'вход не выполнен',
  pairing_required: 'ожидает сопряжения',
  no_account: 'аккаунта ещё нет',
  locked: 'заблокировано',
  root_required: 'нужен корень контента',
  unlocked: 'разблокировано'
}

const ru: Text = {
  state: (state: string) => ruStates[state] ?? state,
  waiting: (n: number) => `Ожидают подтверждения: ${n}`,
  tooltip: (version: string, state: string, n: number) => `dMsg ${version} — ${state}${n ? `, ожидают: ${n}` : ''}`,
  open: 'Открыть dMsg',
  lock: 'Заблокировать',
  openAtLogin: 'Открывать при входе в систему',
  quit: 'Выйти',
  checkUpdates: 'Проверить обновления',
  checking: 'Проверка обновлений…',
  downloading: 'Загрузка обновления…',
  restartToUpdate: (version: string) => `Перезапустить и обновить до ${version}`,
  requestWaiting: 'Запрос ожидает вашего подтверждения.',
  eventsArrived: (n: number) => `Ваши агенты получили новые события: ${n}.`,
  refreshDue: 'Подписку агента на события пора продлить.',
  updateLater: 'dMsg обновится, когда завершатся текущие операции',
  updateBusy: (busy: string) =>
    `Ещё выполняется: ${busy}. Разблокируйте dMsg, чтобы операции завершились, затем снова выберите «Перезапустить и обновить».`,
  inFlightApprovals: (n: number) => `одобренные запросы без окончательного результата: ${n}`,
  inFlightAccount: 'обновление аккаунта с неизвестным результатом',
  inFlightRoot: 'смена корня',
  separator: '; ',
  allFiles: 'Все файлы',
  migrationFile: 'Миграция dMsg',
  frozenFile: 'Замороженная история',
  bundleFile: 'Пакет заверения',
  pickBundle: 'Выберите пакет для проверки',
  pickOriginal: 'Выберите исходный файл пакета',
  pickAction: 'Выберите действие, подготовленное приложением (CBOR)',
  pickAvatar: 'Выберите публичный аватар (PNG, JPEG или WebP, не более 512 КиБ)',
  imageFile: 'Изображения',
  newMessages: 'Новые сообщения в каналах dMsg.',
  newLetter: 'В dMsg пришло новое письмо.'
}

const arStates: Record<string, string> = {
  connecting: 'جارٍ الاتصال',
  unknown: 'غير معروف',
  logged_out: 'لم يُسجَّل الدخول',
  pairing_required: 'بانتظار الإقران',
  no_account: 'لا يوجد حساب بعد',
  locked: 'مقفل',
  root_required: 'يلزم جذر المحتوى',
  unlocked: 'مفتوح'
}

const ar: Text = {
  state: (state: string) => arStates[state] ?? state,
  waiting: (n: number) => `طلبات بانتظار التأكيد: ${n}`,
  tooltip: (version: string, state: string, n: number) => `dMsg ${version} — ${state}${n ? `، بانتظار التأكيد: ${n}` : ''}`,
  open: 'فتح dMsg',
  lock: 'قفل',
  openAtLogin: 'الفتح عند تسجيل الدخول',
  quit: 'إنهاء',
  checkUpdates: 'البحث عن تحديثات',
  checking: 'جارٍ البحث عن تحديثات…',
  downloading: 'جارٍ تنزيل التحديث…',
  restartToUpdate: (version: string) => `إعادة التشغيل للتحديث إلى ${version}`,
  requestWaiting: 'هناك طلب بانتظار موافقتك.',
  eventsArrived: (n: number) => `تلقّى وكلاؤك أحداثًا جديدة: ${n}.`,
  refreshDue: 'حان وقت تجديد اشتراك أحد الوكلاء في الأحداث.',
  updateLater: 'سيُحدَّث dMsg بعد اكتمال العمليات الجارية',
  updateBusy: (busy: string) => `لا يزال قيد التنفيذ: ${busy}. افتح قفل dMsg ليُكملها، ثم اختر «إعادة التشغيل للتحديث» مرة أخرى.`,
  inFlightApprovals: (n: number) => `طلبات تمت الموافقة عليها ولم تُحسم نتيجتها: ${n}`,
  inFlightAccount: 'تحديث للحساب نتيجته غير معروفة',
  inFlightRoot: 'تدوير للجذر',
  separator: '، ',
  allFiles: 'كل الملفات',
  migrationFile: 'ملف ترحيل dMsg',
  frozenFile: 'السجل المجمَّد',
  bundleFile: 'حزمة الإثبات',
  pickBundle: 'اختر الحزمة المراد التحقق منها',
  pickOriginal: 'اختر الملف الأصلي للحزمة',
  pickAction: 'اختر الإجراء الذي أعدّه التطبيق (CBOR)',
  pickAvatar: 'اختر صورة رمزية عامة (PNG أو JPEG أو WebP، بحد أقصى 512 كيبيبايت)',
  imageFile: 'الصور',
  newMessages: 'رسائل جديدة في قنوات dMsg.',
  newLetter: 'وصلت رسالة جديدة إلى dMsg.'
}

const frStates: Record<string, string> = {
  connecting: 'connexion',
  unknown: 'inconnu',
  logged_out: 'déconnecté',
  pairing_required: 'en attente d’appairage',
  no_account: 'pas encore de compte',
  locked: 'verrouillé',
  root_required: 'racine de contenu requise',
  unlocked: 'déverrouillé'
}

const fr: Text = {
  state: (state: string) => frStates[state] ?? state,
  waiting: (n: number) => `${n} demande(s) en attente`,
  tooltip: (version: string, state: string, n: number) => `dMsg ${version} — ${state}${n ? `, ${n} en attente` : ''}`,
  open: 'Ouvrir dMsg',
  lock: 'Verrouiller',
  openAtLogin: 'Ouvrir à la connexion',
  quit: 'Quitter',
  checkUpdates: 'Rechercher des mises à jour',
  checking: 'Recherche de mises à jour…',
  downloading: 'Téléchargement de la mise à jour…',
  restartToUpdate: (version: string) => `Redémarrer et mettre à jour vers ${version}`,
  requestWaiting: 'Une demande attend votre approbation.',
  eventsArrived: (n: number) => `Vos agents ont reçu ${n} nouvel(s) événement(s).`,
  refreshDue: 'L’abonnement aux événements d’un agent doit être renouvelé.',
  updateLater: 'dMsg se mettra à jour une fois les opérations en cours terminées',
  updateBusy: (busy: string) =>
    `Encore en cours : ${busy}. Déverrouillez dMsg pour qu’il termine, puis choisissez à nouveau « Redémarrer et mettre à jour ».`,
  inFlightApprovals: (n: number) => `${n} demande(s) approuvée(s) dont le résultat n’est pas établi`,
  inFlightAccount: 'une mise à jour du compte dont le résultat est inconnu',
  inFlightRoot: 'une rotation de la racine',
  separator: ', ',
  allFiles: 'Tous les fichiers',
  migrationFile: 'Migration dMsg',
  frozenFile: 'Historique gelé',
  bundleFile: 'Paquet d’attestation',
  pickBundle: 'Choisissez le paquet à vérifier',
  pickOriginal: 'Choisissez le fichier d’origine du paquet',
  pickAction: 'Choisissez l’action préparée par l’application (CBOR)',
  pickAvatar: 'Choisissez un avatar public (PNG, JPEG ou WebP, 512 Kio au plus)',
  imageFile: 'Images',
  newMessages: 'Nouveaux messages de canal dans dMsg.',
  newLetter: 'Une nouvelle lettre est arrivée dans dMsg.'
}

const esStates: Record<string, string> = {
  connecting: 'conectando',
  unknown: 'desconocido',
  logged_out: 'sin sesión',
  pairing_required: 'esperando vinculación',
  no_account: 'aún sin cuenta',
  locked: 'bloqueado',
  root_required: 'se necesita la raíz de contenido',
  unlocked: 'desbloqueado'
}

const es: Text = {
  state: (state: string) => esStates[state] ?? state,
  waiting: (n: number) => `${n} solicitud(es) en espera`,
  tooltip: (version: string, state: string, n: number) => `dMsg ${version} — ${state}${n ? `, ${n} en espera` : ''}`,
  open: 'Abrir dMsg',
  lock: 'Bloquear',
  openAtLogin: 'Abrir al iniciar sesión',
  quit: 'Salir',
  checkUpdates: 'Buscar actualizaciones',
  checking: 'Buscando actualizaciones…',
  downloading: 'Descargando la actualización…',
  restartToUpdate: (version: string) => `Reiniciar y actualizar a ${version}`,
  requestWaiting: 'Una solicitud espera tu aprobación.',
  eventsArrived: (n: number) => `Tus agentes recibieron ${n} evento(s) nuevo(s).`,
  refreshDue: 'Hay que renovar la suscripción a eventos de un agente.',
  updateLater: 'dMsg se actualizará cuando terminen las operaciones en curso',
  updateBusy: (busy: string) => `Aún en curso: ${busy}. Desbloquea dMsg para que termine y vuelve a elegir «Reiniciar y actualizar».`,
  inFlightApprovals: (n: number) => `${n} solicitud(es) aprobada(s) con un resultado sin confirmar`,
  inFlightAccount: 'una actualización de la cuenta con resultado desconocido',
  inFlightRoot: 'una rotación de la raíz',
  separator: ', ',
  allFiles: 'Todos los archivos',
  migrationFile: 'Migración de dMsg',
  frozenFile: 'Historial congelado',
  bundleFile: 'Paquete de atestación',
  pickBundle: 'Elige el paquete que quieres verificar',
  pickOriginal: 'Elige el archivo original del paquete',
  pickAction: 'Elige la acción que preparó la aplicación (CBOR)',
  pickAvatar: 'Elige un avatar público (PNG, JPEG o WebP, 512 KiB como máximo)',
  imageFile: 'Imágenes',
  newMessages: 'Mensajes nuevos en los canales de dMsg.',
  newLetter: 'Llegó una carta nueva a dMsg.'
}

const texts: Record<Locale, Text> = { en, zh, ru, ar, fr, es }

export type { Text }

/** The table for a locale such as `fr` or `zh-CN`; English for any other. */
export function textFor(locale: string): Text {
  return texts[matchLocale(locale) ?? defaultLocale]
}
