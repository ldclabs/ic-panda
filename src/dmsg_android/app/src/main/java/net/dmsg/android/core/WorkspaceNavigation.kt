package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*

enum class Workspace(val route: String) { VAULT("vault"), MESSAGES("messages"), FILES("files"), ACCOUNT("account") }
data class Route(val page: String, val id: String? = null, val folder: String = "root", val revision: String? = null, val field: String? = null,
    val channel: String? = null, val sequence: ULong? = null)
data class NavigationState(val workspace: Workspace = Workspace.VAULT,
    val stacks: Map<Workspace, List<Route>> = Workspace.entries.associateWith { listOf(Route(it.route)) }) {
    val stack: List<Route> get() = stacks.getValue(workspace)
    val current: Route get() = stack.last()
    val canGoBack: Boolean get() = stack.size > 1
}

/** Four independent stacks, containing only opaque navigation IDs, never form data. */
class WorkspaceNavigation {
    private var account: String? = null
    private val accounts = mutableMapOf<String, NavigationState>()
    data class ScrollPosition(val first: Int = 0, val offset: Int = 0)
    private val positions = mutableMapOf<String, LinkedHashMap<String, ScrollPosition>>()
    private val mutable = MutableStateFlow(NavigationState())
    val state = mutable.asStateFlow()
    fun scrollOwner(): String? = account
    fun savedScrollPosition(key: String): ScrollPosition? = positions[account]?.get(key)
    fun scrollPosition(key: String): ScrollPosition = savedScrollPosition(key) ?: ScrollPosition()
    fun rememberScroll(owner: String?, key: String, position: ScrollPosition) {
        if (owner == null || owner != account && owner !in accounts || key.length > 160 || position.first < 0 || position.offset < 0) return
        val saved = positions.getOrPut(owner) { linkedMapOf() }
        saved.remove(key); saved[key] = position
        while (saved.size > 32) saved.remove(saved.keys.first())
    }
    fun activate(next: String?) {
        if (next == account) return
        account?.let { accounts[it] = mutable.value }
        account = next
        mutable.value = next?.let { accounts[it] } ?: NavigationState()
    }
    fun forget(id: String) {
        accounts.remove(id)
        positions.remove(id)
        if (account == id) { account = null; mutable.value = NavigationState() }
    }
    fun open(page: String) {
        val workspace = Workspace.entries.firstOrNull { it.route == page }
        if (workspace != null) mutable.value = mutable.value.copy(workspace = workspace)
        else push(Route(page))
    }
    fun select(id: String) {
        if (id.isEmpty()) { back(); return }
        val state = mutable.value
        val route = state.current.copy(id = id)
        val old = if (state.current.id != null) state.stack.dropLast(1) else state.stack
        setStack(old + route)
    }
    fun folder(id: String) {
        val route = Route("files", folder = id)
        val existing = mutable.value.stack.indexOfLast { it == route }
        if (existing >= 0) setStack(mutable.value.stack.take(existing + 1)) else push(route)
    }
    fun back() { if (mutable.value.canGoBack) setStack(mutable.value.stack.dropLast(1)) }
    fun push(route: Route) {
        if (mutable.value.current != route) {
            val next = mutable.value.stack + route
            setStack(if (next.size <= 32) next else listOf(next.first()) + next.takeLast(31))
        }
    }
    private fun setStack(stack: List<Route>) {
        mutable.value = mutable.value.copy(stacks = mutable.value.stacks + (mutable.value.workspace to stack))
    }
    fun save(): String {
        account?.let { accounts[it] = mutable.value }
        return buildJsonObject {
            put("v", 2)
            putJsonObject("accounts") { accounts.forEach { (id, state) ->
                putJsonObject(id) {
                    put("workspace", state.workspace.route)
                    putJsonObject("positions") { positions[id]?.forEach { (key, position) ->
                        putJsonArray(key) { add(position.first); add(position.offset) }
                    } }
                    putJsonObject("stacks") { state.stacks.forEach { (workspace, stack) ->
                        putJsonArray(workspace.route) { stack.forEach { route -> add(buildJsonObject {
                            put("page", route.page); route.id?.let { put("id", it) }; put("folder", route.folder); route.revision?.let { put("revision", it) }; route.field?.let { put("field", it) }
                            route.channel?.let { put("channel", it) }; route.sequence?.let { put("sequence", JsonPrimitive(it.toString().toBigInteger())) }
                        }) } }
                    } }
                }
            } }
        }.toString()
    }
    fun restore(saved: String?) {
        if (saved == null || saved.length > 64 * 1024 || account != null) return
        runCatching {
            val root = Json.parseToJsonElement(saved).objectValue()
            check(root.requiredUnsigned("v") == 2uL)
            val snapshots = root["accounts"]!!.objectValue()
            check(snapshots.size <= 32)
            val restored = snapshots.mapValues { (_, value) ->
                val row = value.objectValue()
                val workspace = Workspace.entries.single { it.route == row.required("workspace") }
                val stacks = row["stacks"]!!.objectValue()
                NavigationState(workspace, Workspace.entries.associateWith { entry ->
                    stacks.rows(entry.route).map { route -> Route(route.required("page"), route.text("id"), route.required("folder"), route.text("revision"), route.text("field"), route.text("channel"), route.unsigned("sequence")) }
                        .also { check(it.isNotEmpty() && it.size <= 32 && it.first() == Route(entry.route) && it.all { route -> route.page.length <= 64 && (route.id?.length ?: 0) <= 128 && route.folder.length <= 128 && (route.revision?.length ?: 0) <= 128 && (route.field?.length ?: 0) <= 128 && (route.channel?.length ?: 0) <= 128 }) }
                })
            }
            val restoredPositions = snapshots.mapValues { (_, value) ->
                val entries = value.objectValue()["positions"]!!.objectValue()
                check(entries.size <= 32)
                LinkedHashMap(entries.mapValues { (key, raw) ->
                    val pair = raw.jsonArray
                    check(key.length <= 160 && pair.size == 2)
                    val first = pair[0].jsonPrimitive.int
                    val offset = pair[1].jsonPrimitive.int
                    check(first >= 0 && offset >= 0)
                    ScrollPosition(first, offset)
                })
            }
            accounts.putAll(restored)
            positions.putAll(restoredPositions)
        }
    }
}
