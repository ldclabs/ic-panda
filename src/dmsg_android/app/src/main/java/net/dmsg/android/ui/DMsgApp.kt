package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.platform.str
import net.dmsg.android.platform.localizedNumber
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.*
import androidx.compose.animation.core.SeekableTransitionState
import androidx.compose.animation.core.rememberTransition
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.core.*
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun DMsgApp(client: ClientController, browser: (String) -> Unit, openLogin: (String) -> Unit, share: (File, String) -> Unit) {
    val state by client.state.collectAsStateWithLifecycle()
    val joiningAccount by client.joiningAccount.collectAsStateWithLifecycle()
    val local = state.session?.accountKind == "local"
    val destinations = if (local) workspaceDestinations.filter { it.id != "messages" } else workspaceDestinations
    LaunchedEffect(state.unlocked, state.concealed, state.session?.account) { client.routePendingChannel(); client.routePendingConnect(); client.routePendingWorkspace(); client.routePendingNotice() }
    val context = LocalContext.current
    val channelListing by client.messages.channels.state.collectAsStateWithLifecycle()
    val syncStatus by client.synchronization.status.collectAsStateWithLifecycle()
    val noticeStatus by client.notifications.status.state.collectAsStateWithLifecycle()
    LaunchedEffect(state.unlocked, state.concealed, state.epoch) { if (state.unlocked && !local && !state.concealed) while (true) { client.notifications.status(); kotlinx.coroutines.delay(30_000) } }
    val navigation by client.navigation.state.collectAsStateWithLifecycle()
    val fold = rememberWorkspaceFold()
    val route = navigation.current.page
    LaunchedEffect(route) { if (route !in setOf("vault-backup", "approvals")) client.vaultBackup.clear() }
    val selected = navigation.current.id.orEmpty()
    val showEditor by client.vaultDrafts.presented.collectAsStateWithLifecycle()
    var discardEditor by remember { mutableStateOf(false) }
    var activityMenu by remember { mutableStateOf(false) }
    val snackbar = remember { SnackbarHostState() }
    fun go(next: String) { client.navigation.open(next); client.refresh(next) }
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    fun select(id: String) {
        keyboard?.hide()
        focus.clearFocus()
        client.navigation.select(id)
    }
    fun closeEditor() { if (client.vaultDrafts.dirty) discardEditor = true else client.vaultDrafts.discard() }
    val passwordImport = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument(), client.vaultImport::result)
    LaunchedEffect(state.epoch) { discardEditor = false }
    LaunchedEffect(state.unlocked, state.concealed, state.session?.account) {
        if (state.unlocked && !state.concealed) client.vaultDrafts.load()
    }
    LaunchedEffect(state.unlocked) { if (state.unlocked) client.refresh(route) }
    DisposableEffect(navigation.current, state.epoch, state.unlocked, state.concealed) {
        onDispose { snackbar.currentSnackbarData?.dismiss() }
    }
    LaunchedEffect(state.error, state.notice) {
        (state.error ?: state.notice)?.let { snackbar.showSnackbar(errorText(it)); client.clearNotice() }
    }
    val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
    BackHandler(enabled = showEditor) { closeEditor() }
    val screenChrome = remember { ScreenChrome() }
    val filePickers = rememberFilePickers(client)
    val noticePermission = rememberNoticePermission(client)
    CompositionLocalProvider(LocalFilePickers provides filePickers, LocalNoticePermission provides noticePermission) {
    DMsgTheme(state.dark) {
        Surface(Modifier.fillMaxSize().fileDropTarget(client, !state.concealed && (!state.unlocked || navigation.workspace == Workspace.FILES))) {
            when {
                state.starting -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Image(painterResource(R.drawable.dmsg_mark), null, Modifier.size(52.dp)) }
                joiningAccount || (state.unlocked && route == "account-invite") -> Box(Modifier.fillMaxSize()) {
                    if (joiningAccount) AccountJoinScreen(client, openLogin)
                    else Column(Modifier.fillMaxSize().systemBarsPadding()) {
                        // Keep the invitation mounted while Android shows the system credential prompt.
                        BackHandler { client.navigation.open("devices") }
                        if (!state.concealed) TextButton(onClick = { client.navigation.open("devices") }) {
                            Text(str(R.string.text_back_5db5cac))
                        }
                        Box(Modifier.weight(1f)) { AccountInviteHostScreen(client) }
                    }
                    if (!state.concealed) {
                        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).navigationBarsPadding().imePadding())
                    }
                }
                state.concealed -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Image(painterResource(R.drawable.dmsg_mark), null, Modifier.size(52.dp)) }
                !state.unlocked -> EntryScreen(client, state, browser, openLogin, snackbar)
                else -> BoxWithConstraints {
                    val density = LocalDensity.current
                    val separatedPanesFit = fold?.let { feature -> with(density) {
                        feature.vertical && feature.bounds.left.toDp() >= 320.dp && maxWidth - feature.bounds.right.toDp() >= 320.dp
                    } } ?: true
                    val wide = maxWidth >= 840.dp && density.fontScale < 1.6f && separatedPanesFit
                    val root = route in destinations.map { it.id }
                    val inConversation = route == "messages" && selected.isNotEmpty()
                    var threadSearch by remember(state.epoch, route, selected) { mutableStateOf(BrowseSearchMode.Browse) }
                    val scopeList = selected.isEmpty() && when (route) {
                        "inbox" -> navigation.stack.getOrNull(navigation.stack.lastIndex - 1)?.page == "messages"
                        "cloud-files" -> navigation.stack.getOrNull(navigation.stack.lastIndex - 1)?.page == "files"
                        else -> false
                    }
                    val navigationBar = !wide && (root || scopeList) && selected.isEmpty() && !imeVisible
                    val browseRoutes = setOf("vault", "messages", "inbox", "files", "file-trash", "cloud-files")
                    val backButton: @Composable () -> Unit = {
                        IconButton(onClick = client.navigation::back) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, str(R.string.text_back_5db5cac)) }
                    }
                    val activityButton: @Composable () -> Unit = {
                        if (!local) Box {
                            IconButton(onClick = { activityMenu = true }) { BadgedBox(badge = { if ((noticeStatus.valueOrNull()?.counts?.total ?: 0u) > 0u || (state.session?.approvalsWaiting ?: 0u) > 0u || syncStatus?.error != null) Badge() }) { Icon(Icons.Outlined.NotificationsNone, str(R.string.text_activity_1d8edfa)) } }
                            DropdownMenu(activityMenu, { activityMenu = false }) {
                                val reminders = noticeStatus.valueOrNull()?.counts?.total
                                val approvals = state.session?.approvalsWaiting
                                val actions = listOf(
                                    Triple("approvals", str(R.string.text_approvals_1b5e94c) + " · " + (approvals?.let { localizedNumber(it) } ?: "—"), (approvals ?: 0u) > 0u),
                                    Triple("reminders", str(R.string.text_reminders_e43c7b0) + " · " + (reminders?.let { localizedNumber(it) } ?: "—"), (reminders ?: 0uL) > 0uL),
                                    Triple("sync", str(R.string.text_account_synchronization_77ccb72), syncStatus?.error != null),
                                )
                                actions.sortedByDescending { it.third }.forEach { (page, label, _) ->
                                    DropdownMenuItem(text = { Text(label) }, onClick = { activityMenu = false; go(page) })
                                }
                                DropdownMenuItem(text = { Text(str(R.string.text_activity_on_this_device_288740a)) }, onClick = { activityMenu = false; go("secret-activity") })
                            }
                        }
                    }
                    // Every page brings its own bar, so a transition or a back gesture moves the title with the page.
                    val pageFrame: @Composable (PageKey, @Composable () -> Unit) -> Unit = { key, content ->
                        val page = key.route
                        val id = page.id.orEmpty()
                        val current = page == navigation.current
                        val conversation = page.page == "messages" && id.isNotEmpty()
                        val browsing = !wide && id.isEmpty() && page.page in browseRoutes
                        val bar = !browsing && (wide || !conversation || !current || threadSearch == BrowseSearchMode.Browse)
                        val back = if (key.depth > 1) backButton else null
                        val parent = navigation.stacks.getValue(key.workspace).getOrNull(key.depth - 2)?.page
                        // The activity bell belongs to workspace roots and the scopes beside them.
                        val pageRoot = page.page in destinations.map { it.id }
                        val rootLike = pageRoot && (wide || id.isEmpty()) || id.isEmpty() && (page.page == "inbox" && parent == "messages" || page.page == "cloud-files" && parent == "files")
                        val pageBar = remember { PageBar() }
                        val collapse = TopAppBarDefaults.exitUntilCollapsedScrollBehavior(rememberTopAppBarState())
                        Column(Modifier.fillMaxSize().then(if (bar) Modifier.nestedScroll(collapse.nestedScrollConnection) else Modifier.statusBarsPadding())) {
                            if (bar) {
                                val label = pageBar.title ?: when {
                                    local && page.page == "account" -> str(R.string.offline_account)
                                    conversation -> channelListing.valueOrNull()?.find { it.id == id }?.name ?: str(R.string.text_conversation_654b88b)
                                    // A vault item names itself through ScreenBar.
                                    page.page == "vault" && id.isNotEmpty() && !wide -> ""
                                    current && scopeList -> str(destinations.first { it.id == key.workspace.route }.label)
                                    pageRoot -> str(destinations.first { it.id == page.page }.label)
                                    else -> title(page.page)
                                }
                                val title: @Composable () -> Unit = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                                val navigationIcon: @Composable () -> Unit = { back?.invoke() }
                                val actions: @Composable RowScope.() -> Unit = {
                                    if (!local && page.page == "account") IconButton(onClick = { client.lock() }) { Icon(Icons.Outlined.Lock, str(R.string.text_lock_workspace_78665ed)) }
                                    pageBar.actions?.invoke(this)
                                    if (rootLike) activityButton()
                                }
                                val colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.page,
                                    scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer)
                                // Wide roots keep one bar over both panes and a conversation or an open file stays compact;
                                // other pages, a vault item included, show a large title that collapses.
                                if (pageRoot && (wide || conversation || page.page == "files" && id.isNotEmpty())) TopAppBar(title, navigationIcon = navigationIcon, actions = actions, colors = colors)
                                else LargeTopAppBar(title, navigationIcon = navigationIcon, actions = actions, colors = colors, scrollBehavior = collapse)
                            }
                            if (conversation && current) {
                                val syncError by client.messages.channelSyncError.collectAsStateWithLifecycle()
                                val headerNotice = syncError?.takeIf { it.first == id }?.let { errorText(it.second) }
                                SnackbarHost(snackbar) { notice ->
                                    if (notice.visuals.message != headerNotice) Snackbar(notice)
                                }
                            }
                            CompositionLocalProvider(LocalBrowseChrome provides BrowseChrome(if (bar) null else back, if (rootLike && !bar) activityButton else { {} }), LocalPageBar provides pageBar) {
                                Box(Modifier.weight(1f).fillMaxWidth()) { content() }
                            }
                        }
                    }
                    FoldSafeWorkspace(fold, wide && (route in setOf("vault", "messages", "files") || navigation.workspace == Workspace.ACCOUNT)) {
                    Row(Modifier.fillMaxSize().testTag("workspace-content")) {
                        if (wide) WorkspaceNavigationRail(destinations, navigation.workspace.route, ::go)
                        Scaffold(
                            modifier = Modifier.weight(1f).imePadding(),
                            containerColor = MaterialTheme.colorScheme.page,
                            contentWindowInsets = ScaffoldDefaults.contentWindowInsets.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom),
                            bottomBar = { if (navigationBar) WorkspaceNavigationBar(destinations, navigation.workspace.route, ::go) },
                            floatingActionButton = { screenChrome.fab?.invoke() },
                            snackbarHost = { if (!inConversation) SnackbarHost(snackbar) },
                        ) { padding ->
                            WorkspaceBody(padding) {
                                val pageContent: @Composable (Route) -> Unit = { page ->
                                    val route = page.page
                                    val selected = page.id.orEmpty()
                                    val inConversation = route == "messages" && selected.isNotEmpty()
                                    when (route) {
                                    "vault" -> {
                                        if (wide) WorkspacePanes(fold, list = { VaultScreen(client, ::select) }, detail = {
                                            if (selected.isNotEmpty()) DetailPane { SecretScreen(client, selected) { client.vaultDrafts.open(it) } } else EmptyState(str(R.string.text_choose_an_item_3782357), glyph = R.drawable.glyph_vault)
                                        }) else if (selected.isNotEmpty()) SecretScreen(client, selected) { client.vaultDrafts.open(it) }
                                        else VaultScreen(client, ::select)
                                    }
                                    "messages" -> {
                                        val list: @Composable () -> Unit = { MessagesScreen(client, state, ::select) { go("inbox") } }
                                        val detail: @Composable () -> Unit = {
                                            if (selected.isEmpty()) EmptyState(str(R.string.text_choose_a_conversation_f1e5111), glyph = R.drawable.glyph_channels)
                                            else ConversationScreen(client, state, selected, threadSearch) { threadSearch = it }
                                        }
                                        if (wide) WorkspacePanes(fold, list = list, detail = detail)
                                        else if (inConversation) detail() else list()
                                    }
                                    "inbox" -> InboxScreen(client, state, page, showScopes = scopeList)
                                    "invite-letter-sender" -> InviteLetterSenderScreen(client, selected)
                                    "message-settings" -> MessageSettingsScreen(client)
                                    "inbox-settings" -> InboxSettingsScreen(client)
                                    "blocked-accounts" -> BlockedAccountsScreen(client, page.id)
                                    "reports" -> ReportsScreen(client, page.id)
                                    "report-compose" -> ReportComposeScreen(client, ReportTarget(page.field.orEmpty(), selected, page.channel.orEmpty(), page.sequence))
                                    "channel-settings" -> ChannelSettingsScreen(client, page.channel.orEmpty())
                                    "channel-setup" -> ChannelSetupScreen(client)
                                    "channel-invite" -> ChannelInviteScreen(client, page.channel.orEmpty())
                                    "channel-hidden" -> HiddenChannelsScreen(client)
                                    "message-reference" -> ReferencedMessageScreen(client, page.channel.orEmpty(), page.sequence ?: 0u)
                                    "files" -> if (wide) WorkspacePanes(fold, list = { FilesScreen(client, state, page.copy(id = null, revision = null), ::select, share) }, detail = {
                                        if (selected.isNotEmpty()) FilesScreen(client, state, page, ::select, share) else EmptyState(str(R.string.text_choose_a_file_2916860), glyph = R.drawable.glyph_file)
                                    }) else FilesScreen(client, state, page, ::select, share)
                                    "incoming-shares" -> IncomingSharesScreen(client)
                                    "workspace-entries" -> WorkspaceEntriesScreen()
                                    "system-autofill" -> SystemExportsScreen(client, net.dmsg.android.platform.ExportPurpose.credentials)
                                    "system-files" -> SystemExportsScreen(client, net.dmsg.android.platform.ExportPurpose.files)
                                    "file-trash" -> FilesScreen(client, state, page, ::select, share, trash = true)
                                    "approvals" -> ApprovalsScreen(client, state, selected, ::select)
                                    "account" -> if (local) LocalSettings(client, ::go) else AccountScreen(client, state, ::go, openLogin)
                                    "system-integrations" -> SystemIntegrationsScreen(client)
                                    "advanced" -> AdvancedAccountScreen(client)
                                    "about" -> AboutScreen(browser) { client.navigation.push(Route("privacy")) }
                                    "privacy" -> PrivacyScreen(browser)
                                    "profile" -> PublicProfileScreen(client)
                                    "browser-connections" -> BrowserConnectionsScreen(client, browser)
                                    "browser-request" -> {
                                        val link = ConnectLink(page.field.orEmpty(), page.channel, page.id)
                                        if (ConnectLink.parse(link.url()) == link) BrowserRequestScreen(client, link, browser) else EmptyState(str(R.string.text_invalid_request_link_0f99d32))
                                    }
                                    "device-transfer" -> DeviceTransferScreen(client, page.id, page.field?.let { it == "predecessor" })
                                    "handovers" -> HandoversScreen(client, page.id)
                                    "handover-review" -> HandoverReviewScreen(client)
                                    "agents" -> AgentsScreen(client)
                                    "agent" -> AgentDetailScreen(client, selected)
                                    "agent-delegations" -> AgentDelegationsScreen(client)
                                    "agent-controller" -> AgentDelegationFormScreen(client, controller = true)
                                    "agent-grant" -> AgentDelegationFormScreen(client, controller = false)
                                    "wallets" -> WalletsScreen(client)
                                    "wallet" -> WalletScreen(client, selected)
                                    "wallet-transfer" -> WalletTransferScreen(client, selected)
                                    "wallet-proposal" -> WalletProposalScreen(client, selected, page.sequence ?: 0u)
                                    "wallet-controls" -> WalletControlsScreen(client, selected)
                                    "wallet-cosigners" -> WalletCosignersScreen(client)
                                    "wallet-keys" -> WalletDeviceKeysScreen(client, selected)
                                    "wallet-native-move" -> WalletNativeMoveScreen(client, selected, (page.sequence ?: 0u).toUInt())
                                    "membership" -> MembershipScreen(client)
                                    "membership-compare" -> MembershipScreen(client, comparison = true)
                                    "accounts" -> AccountsScreen(client)
                                    "display-cache" -> DisplayCacheScreen(client)
                                    "account-remove" -> RemoveLocalAccountScreen(client)
                                    "account-delete" -> AccountDeletionScreen(client)
                                    "security" -> SecurityScreen(client, state)
                                    "devices" -> DevicesScreen(client)
                                    "recovery" -> RecoveryScreen(client, page.id)
                                    "recovery-policy" -> RecoveryPolicyScreen(client)
                                    "bindings" -> LoginIdentitiesScreen(client, openLogin)
                                    "hardware" -> HardwareScreen(client)
                                    "pair-approve" -> PairApprovalScreen(client)
                                    "key-enrollment" -> PairApprovalScreen(client, enrollmentOnly = true)
                                    "notifications" -> NotificationsScreen(client)
                                    "notification-timezone" -> NotificationTimezoneScreen(client)
                                    "reminders" -> RemindersScreen(client)
                                    "sync" -> SyncScreen(client)
                                    "feedback" -> FeedbackScreen(client)
                                    "file-transfers" -> TransferScreen(client)
                                    "file-preview" -> FilePreviewScreen(client, selected, page.revision.orEmpty())
                                    "cloud-files" -> CloudFilesScreen(client, selected, showScopes = scopeList)
                                    "file-unbacked" -> UnbackedFilesScreen(client)
                                    "file-backup" -> if (page.field == "batch") BatchBackupScreen(client)
                                        else FileBackupScreen(client, selected, page.revision, page.field == "revision")
                                    "file-requests" -> FileRequestsScreen(client, selected)
                                    "file-request-compose" -> FileRequestComposerScreen(client)
                                    "file-request-batch" -> FileRequestBatchScreen(client, selected)
                                    "file-links" -> FileLinksScreen(client, selected)
                                    "vault-links" -> FileLinksScreen(client, "", snapshots = true, vaultItem = page.id)
                                    "file-link-job" -> FileLinkJobScreen(client, selected)
                                    "file-link-compose" -> FileLinkComposerScreen(client, selected, page.revision, false)
                                    "backup-link-compose" -> FileLinkComposerScreen(client, selected, null, true)
                                    "vault-history" -> VaultHistoryScreen(client, selected)
                                    "vault-version" -> HistoricalSecretScreen(client, selected, page.revision.orEmpty())
                                    "vault-compare" -> VaultComparisonScreen(client, selected, page.revision.orEmpty())
                                    "conflicts" -> VaultConflictsScreen(client, selected, ::select)
                                    "secret-attachments" -> SecretAttachmentsScreen(client, selected)
                                    "secret-reference" -> SecretReferenceScreen(client, selected, page.field.orEmpty())
                                    "secret-health" -> SecretHealthScreen(client)
                                    "secret-rotation" -> SecretRotationScreen(client, selected)
                                    "secret-activity" -> SecretActivityScreen(client)
                                    "secret-compose" -> SecretSnapshotComposer(client, selected, page.channel)
                                    "secret-shares" -> SecretSharesScreen(client)
                                    "secret-share" -> SecretShareScreen(client, selected)
                                    "secret-import" -> SecretPackageScreen(client, page.channel, page.sequence)
                                    "secret-picker" -> SecretSnapshotPicker(client, page.channel.orEmpty())
                                    "vault-import" -> VaultImportScreen(client) { passwordImport.launch(arrayOf("*/*")) }
                                    "local-import" -> LocalImportScreen(client)
                                    "vault-backup" -> VaultBackupScreen(client)
                                    else -> EmptyState(str(R.string.text_choose_a_destination_f70f6ae), glyph = R.drawable.glyph_account)
                                } }
                                val framedPage: @Composable (PageKey) -> Unit = { key -> pageFrame(key) { pageContent(key.route) } }
                                Box(Modifier.weight(1f).fillMaxWidth()) {
                                    CompositionLocalProvider(LocalScreenChrome provides screenChrome) {
                                        if (!local && wide && navigation.workspace == Workspace.ACCOUNT) WorkspacePanes(fold,
                                            list = { pageFrame(PageKey(Workspace.ACCOUNT, 1, Route("account"))) { AccountScreen(client, state, ::go, openLogin) } },
                                            detail = {
                                                if (route == "account") Box(Modifier.statusBarsPadding()) { EmptyState(str(R.string.text_choose_a_destination_f70f6ae), glyph = R.drawable.glyph_account) }
                                                else AnimatedPage(navigation, !imeVisible, client.navigation::back, itemPages = false, content = framedPage)
                                            })
                                        else AnimatedPage(navigation, !imeVisible, client.navigation::back, itemPages = !wide, content = framedPage)
                                    }
                                    // Work in progress shows over the page instead of reserving a strip on every page.
                                    if (state.busy) LinearProgressIndicator(Modifier.align(Alignment.TopCenter).statusBarsPadding().fillMaxWidth())
                                }
                            }
                        }
                    }
                    }
                }
            }
            NoticeBanner(client)
            NoticeDestinationDialog(client)
            QrScannerHost(client, state.concealed)
            if (showEditor && state.unlocked && !state.concealed) Dialog(onDismissRequest = ::closeEditor,
                properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false, dismissOnClickOutside = false)) {
                Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.page) { VaultEditor(client, close = ::closeEditor) }
            }
            if (discardEditor && state.unlocked && !state.concealed) AlertDialog(onDismissRequest = { discardEditor = false }, title = { Text(str(R.string.text_keep_your_changes_01fdac5)) },
                confirmButton = { TextButton(onClick = { discardEditor = false; client.vaultDrafts.keep() }) { Text(str(R.string.text_keep_draft_89639c8)) } },
                dismissButton = { TextButton(onClick = { client.vaultDrafts.discard(); discardEditor = false }) { Text(str(R.string.text_discard_changes_9c7370b)) } })
        }
    }
    }
}

private data class PageKey(val workspace: Workspace, val depth: Int, val route: Route)

/**
 * Pages move along a shared horizontal axis: forward enters from the end, back from the start;
 * switching workspaces fades through. A page keeps its composition, state and loads while only its
 * selection or folder changes; with [itemPages], a vault item or a conversation, which have screens
 * of their own, opens as a separate page. The back gesture scrubs the back transition, so the
 * previous page shows as the finger moves.
 */
@Composable private fun AnimatedPage(navigation: NavigationState, backEnabled: Boolean, back: () -> Unit,
    itemPages: Boolean, content: @Composable (PageKey) -> Unit) {
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    val distance = with(LocalDensity.current) { 30.dp.roundToPx() }
    val target = PageKey(navigation.workspace, navigation.stack.size, navigation.current)
    val previous = navigation.stack.getOrNull(navigation.stack.lastIndex - 1)?.let { PageKey(navigation.workspace, navigation.stack.lastIndex, it) }
    val pages = remember { SeekableTransitionState(target) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(target) { pages.animateTo(target) }
    PredictiveBackHandler(enabled = backEnabled && previous != null) { progress ->
        val to = checkNotNull(previous)
        try {
            progress.collect { event -> pages.seekTo(event.progress, to) }
            back()
        } catch (cancelled: CancellationException) {
            scope.launch { pages.animateTo(pages.currentState) }
            throw cancelled
        }
    }
    rememberTransition(pages, label = "page").AnimatedContent(
        modifier = Modifier.fillMaxSize(),
        contentKey = { if (itemPages && it.route.page in setOf("vault", "messages")) it else it.workspace to it.route.page },
        transitionSpec = {
            val from = initialState
            val to = targetState
            if (from.workspace != to.workspace || from.depth == to.depth) fadeIn(Motion.effects()) togetherWith fadeOut(Motion.effectsFast())
            else {
                val sign = if ((to.depth > from.depth) != rtl) 1 else -1
                (slideInHorizontally(Motion.spatial()) { sign * distance } + fadeIn(Motion.effects())) togetherWith
                    (slideOutHorizontally(Motion.spatial()) { -sign * distance } + fadeOut(Motion.effectsFast()))
            }
        },
    ) { key -> content(key) }
}

private fun title(route: String) = when (route) {
    "system-integrations" -> str(R.string.text_system_integrations_9ea81ce)
    "advanced" -> str(R.string.text_advanced)
    "about" -> str(R.string.text_about_dmsg_630408f)
    "privacy" -> str(R.string.text_privacy_policy_d216307)
    "reminders" -> str(R.string.text_reminders_e43c7b0)
    "notification-timezone" -> str(R.string.text_time_zone_0298f23)
    "browser-connections" -> str(R.string.text_connected_browsers_e404840)
    "browser-request" -> str(R.string.text_browser_request_0fa188e)
    "incoming-shares" -> str(R.string.text_shared_with_dmsg_0df2fa8)
    "workspace-entries" -> str(R.string.text_home_screen_entries_7e5f087)
    "system-autofill" -> str(R.string.text_password_autofill_872af78)
    "system-files" -> str(R.string.text_files_app_copies_96ebae6)
    "device-transfer" -> str(R.string.text_transfer_between_devices_fc09783)
    "handovers" -> str(R.string.text_replace_a_device_fbe5457)
    "handover-review" -> str(R.string.text_review_replacement_686825b)
    "agents", "agent" -> str(R.string.text_agents_dcfd6ed)
    "agent-delegations" -> str(R.string.text_controllers_and_delegations_8ce7d82)
    "agent-controller" -> str(R.string.text_new_controller_4d1faa5)
    "agent-grant" -> str(R.string.text_grant_delegation_ab521bf)
    "wallets" -> str(R.string.text_multisig_wallets_451404c)
    "wallet" -> str(R.string.text_wallet_77c08be)
    "wallet-transfer" -> str(R.string.text_propose_transfer_275bacf)
    "wallet-proposal" -> str(R.string.text_wallet_proposal_45007ea)
    "wallet-controls" -> str(R.string.text_wallet_controls_8173ebf)
    "wallet-cosigners" -> str(R.string.text_hardware_cosigners_dbe97a0)
    "wallet-keys" -> str(R.string.text_device_wallet_keys_76c4225)
    "wallet-native-move" -> str(R.string.text_move_native_address_f617311)
    "account-delete" -> str(R.string.text_delete_service_account_1475636)
    "membership" -> str(R.string.text_membership_ff9b3ff)
    "membership-compare" -> str(R.string.text_compare_all_plans_a6fbd12)
    "accounts" -> str(R.string.text_switch_account_52f71df)
    "display-cache" -> str(R.string.text_cached_data_b29fb6f)
    "account-remove" -> str(R.string.text_remove_local_data_c45125e)
    "invite-letter-sender" -> str(R.string.text_invite_to_a_conversation_e66d6f9)
    "message-settings" -> str(R.string.text_message_settings_1a28455)
    "inbox-settings" -> str(R.string.text_receiving_policy_7270844)
    "blocked-accounts" -> str(R.string.text_blocked_accounts_c7f6322)
    "reports" -> str(R.string.text_your_reports_8c328aa)
    "report-compose" -> str(R.string.text_report_content_c9655b3)
    "channel-settings" -> str(R.string.text_conversation_settings_f07a685)
    "channel-setup" -> str(R.string.text_new_conversation_8687862)
    "channel-invite" -> str(R.string.text_invite_member_70798d4)
    "channel-hidden" -> str(R.string.text_hidden_conversations_46b7307)
    "message-reference" -> str(R.string.text_original_message_bd62a38)
    "approvals" -> str(R.string.text_approvals_d0130a6); "devices" -> str(R.string.text_your_devices_cc95bcc); "recovery" -> str(R.string.text_account_recovery_04aaf7e)
    "security" -> str(R.string.text_security_c4c7059); "notifications" -> str(R.string.text_notifications_3e7769a); "profile" -> str(R.string.text_public_profile_a67b108)
    "key-enrollment" -> str(R.string.text_review_an_enrollment_request_65ae501)
    "account-invite" -> str(R.string.text_authorize_another_device_d67d951)
    "pair-approve" -> str(R.string.text_add_device_0912d8b)
    "secret-attachments" -> str(R.string.text_item_attachments_e29ea9d); "secret-reference" -> str(R.string.text_field_reference_f38f4f4)
    "secret-health" -> str(R.string.text_local_security_review_b7f4013); "secret-rotation" -> str(R.string.text_credential_rotation_1ec9a4c)
    "secret-activity" -> str(R.string.text_activity_on_this_device_288740a)
    "secret-compose" -> str(R.string.text_share_secret_snapshot_d4484d8); "secret-shares" -> str(R.string.text_shared_snapshots_08679a9)
    "secret-share" -> str(R.string.text_snapshot_853fcbe); "secret-import" -> str(R.string.text_import_secret_package_51433fa); "secret-picker" -> str(R.string.text_choose_a_secret_2ceded8)
    "vault-import" -> str(R.string.text_import_passwords_16088a6)
    "local-import" -> str(R.string.import_local_account)
    "vault-backup" -> str(R.string.vault_backup_title)
    "vault-history" -> str(R.string.text_version_history_8a6bf11); "vault-version" -> str(R.string.text_historical_version_119b50c); "vault-compare" -> str(R.string.text_compare_versions_9f9b2e0)
    "inbox" -> str(R.string.text_letters_cfbff5c)
    "hardware" -> str(R.string.text_hardware_security_04e42f4); "recovery-policy" -> str(R.string.text_on_chain_recovery_dcf7d3f)
    "file-transfers" -> str(R.string.text_transfers_6fdfd95)
    "file-preview" -> str(R.string.text_verified_preview_80df961)
    "cloud-files" -> str(R.string.text_cloud_backups_ab4daf1)
    "file-unbacked" -> str(R.string.without_backup)
    "file-backup" -> str(R.string.text_back_up_to_cloud_ee48ac0)
    "file-requests" -> str(R.string.text_collect_files_3ad17e5)
    "file-request-compose", "file-request-batch" -> str(R.string.text_collection_links_91ac11d)
    "file-links", "vault-links" -> str(R.string.text_share_links_fe69aa5)
    "file-link-job", "file-link-compose", "backup-link-compose" -> str(R.string.text_share_by_link_05aafef)
    "file-trash" -> str(R.string.text_trash_8af1079)
    "sync" -> str(R.string.text_account_synchronization_77ccb72); "feedback" -> str(R.string.text_feedback_3accf75)
    "bindings" -> str(R.string.text_sign_in_identities_aa0f5b7); "conflicts" -> str(R.string.text_version_conflicts_0df1b3c)
    else -> str(R.string.text_inbox_2cd8ee3)
}
