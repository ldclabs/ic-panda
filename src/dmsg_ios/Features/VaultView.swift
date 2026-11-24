import SwiftUI
@preconcurrency import DMsgCore

struct VaultView: View {
    @Environment(SessionModel.self) private var model
    @State private var trash = false
    @State private var filtering = false
    @State private var hasLocalSource = false
    @State private var searchScope: WorkspaceSearchScope = .current
    @State private var globalSearch = GlobalSearchStore()

    private var searchingAll: Bool {
        searchScope == .all && !model.search.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        @Bindable var model = model
        WorkspaceNavigation(selection: $model.route.vaultItem, sidebarInitiallyVisible: true) { selection in
            List(selection: selection) {
                Group {
                if searchingAll {
                    GlobalSearchResultsContent(store: globalSearch, query: model.search)
                } else {
                if model.vaultQueryIsCurrent {
                    ForEach(model.vault, id: \.id) { item in
                        NavigationLink(value: item.id) { VaultRow(item: item, searchMeta: model.search.isEmpty ? nil : model.vaultSearchMeta[item.id]) }
                            .compactObjectRow().accessibilityIdentifier("vault.item.\(item.id)")
                            .swipeActions { if item.kind == "login" { copyActions(item) } }
                            .contextMenu { if item.kind == "login" { copyActions(item) } }
                    }
                }
                if model.loadingVault || !model.vaultQueryIsCurrent && !model.vaultFailed {
                    if model.vaultQueryIsCurrent && !model.vault.isEmpty {
                        ProgressView().frame(maxWidth: .infinity).listRowBackground(Color.clear)
                            .accessibilityIdentifier("vault.loading")
                    } else { PlaceholderRows(identifier: "vault.loading") }
                }
                if model.vaultFailed {
                    NoticeBar(text: String(localized: "The operation could not be completed. Please try again."))
                    Button("Retry") { Task { await model.loadVault() } }.accessibilityIdentifier("vault.retry")
                } else if model.vaultLoaded && model.vaultQueryIsCurrent && model.vault.isEmpty && !model.loadingVault { empty }
                if model.vaultQueryIsCurrent && model.vaultHasMore {
                    LoadMoreRow(working: model.loadingVault, trigger: model.vault.count) { await model.loadVault(more: true) }
                        .accessibilityIdentifier("vault.more")
                }
                }
                }.plainListRows(selectable: selection != nil)
            }.listStyle(.plain).refreshable { await model.synchronize() }
                .globalSearch(store: globalSearch, query: model.search, active: searchingAll)
                .scopeBar { if !searchingAll { VaultBrowseBar(filtering: $filtering) } }
                .privateScreen(.plain).navigationTitle("Vault")
                .searchable(text: $model.search, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search your vault")
                .searchScopes($searchScope) {
                    Text("Current").tag(WorkspaceSearchScope.current)
                    Text("All").tag(WorkspaceSearchScope.all)
                }
                .onChange(of: model.search) { _, value in
                    if value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { searchScope = .current }
                }
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { ActivityButton() }
                    ToolbarItemGroup(placement: .primaryAction) {
                        Button("Add item", systemImage: "plus") { add() }.accessibilityIdentifier("vault.add").keyboardShortcut("n", modifiers: .command)
                        Menu("Vault tools", systemImage: "ellipsis.circle") {
                            Section {
                                NavigationLink { SystemExportsView(section: .autoFill) } label: { Label("Password AutoFill", systemImage: "key") }
                                NavigationLink { SecretHealthView() } label: { Label("Local security review", systemImage: "checkmark.shield") }
                            }
                            Section {
                                NavigationLink { VaultImportView() } label: { Label("Import passwords", systemImage: "square.and.arrow.down") }
                                NavigationLink { VaultBackupView() } label: { Label("Backup and restore", systemImage: "externaldrive") }
                                if model.cloudAccount && hasLocalSource { NavigationLink { LocalImportView() } label: { Label("Import from local account", systemImage: "square.and.arrow.down") } }
                                if !model.localAccount { NavigationLink { SecretPackageImportView() } label: { Label("Import secret package", systemImage: "shippingbox") } }
                                if !model.localAccount { NavigationLink { SecretSharesView() } label: { Label("Shared snapshots", systemImage: "link") } }
                            }
                            Section {
                                NavigationLink { SecretActivityView() } label: { Label("Activity on this device", glyph: .audit) }
                                Button("Trash", systemImage: "trash") { trash = true }
                            }
                        }.accessibilityIdentifier("vault.tools")
                    }
                }
                .task(id: model.session?.generation) {
                    hasLocalSource = false
                    guard model.cloudAccount, let engine = model.engine else { return }
                    if let snapshot = try? await engine.accounts(),
                       let accounts: [AccountsView.LocalAccount] = try? model.decode(snapshot) {
                        hasLocalSource = accounts.contains { $0.accountKind == "local" }
                    }
                }
                .task(id: model.vaultQueryKey) {
                    try? await Task.sleep(for: .milliseconds(200))
                    if !Task.isCancelled { await model.loadVault() }
                }
        } detail: { id in
            if let id { VaultDetailView(id: id) }
            else { ContentUnavailableView("Select a secret", image: AppGlyph.vault.assetName,
                description: Text("Your hidden fields stay concealed until you choose to reveal them.")) }
        }
        .onChange(of: model.route.vaultItem) { _, value in if value != nil { model.activity() } }
        .sheet(isPresented: $trash) { NavigationStack { VaultTrashView() } }
        .sheet(isPresented: $filtering) { NavigationStack { VaultFiltersView() }.presentationDetents([.medium, .large]) }
    }

    @ViewBuilder private func copyActions(_ item: VaultItem) -> some View {
        Button("Copy password", systemImage: "key") { Task { await model.copyVaultField(item, field: "password") } }.tint(Theme.forest)
        Button("Copy username", systemImage: "person") { Task { await model.copyVaultField(item, field: "username") } }
    }

    @ViewBuilder private var empty: some View {
        if !model.localAccount && model.contentSync?.initialized != true {
            ContentUnavailableView(ContentSyncStatusView.title(model.contentSync), systemImage: "arrow.triangle.2.circlepath")
        } else if !model.search.isEmpty {
            ContentUnavailableView.search(text: model.search)
            Button("Clear search") { model.search = "" }
            Button("Add item", systemImage: "plus") { add() }.accessibilityIdentifier("vault.empty.add")
        } else {
            ContentUnavailableView(model.vaultFilter == "favorites" ? "No favorites" : "No items", image: AppGlyph.vault.assetName)
            if model.vaultFilter == "all" && model.vaultTag.isEmpty {
                NavigationLink("Import from backup") { VaultBackupView() }
            }
            Button("Add item", systemImage: "plus") { add() }
            if model.vaultFilter == "all" && model.vaultTag.isEmpty {
                NavigationLink("Import passwords") { VaultImportView() }
            } else { Button("Clear filters") { clear() } }
        }
    }

    private func clear() { model.search = ""; model.vaultTag = ""; model.vaultFilter = "all" }

    private func add() {
        let target = VaultEditorTarget(item: nil, kind: vaultKinds().contains(model.vaultFilter) ? model.vaultFilter : "login", favorite: model.vaultFilter == "favorites", tag: model.vaultTag)
        Task { await model.vaultEditing.open(target, model: model) }
    }
}

private struct VaultBrowseBar: View {
    @Environment(SessionModel.self) private var model
    @Binding var filtering: Bool
    @Environment(\.horizontalSizeClass) private var sizeClass
    private var moreSelected: Bool { vaultKinds().contains(model.vaultFilter) && !["login", "note"].contains(model.vaultFilter) }

    var body: some View {
        HStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        scope("all", title: "All", symbol: "square.grid.2x2")
                        scope("favorites", title: "Favorites", symbol: "star")
                        scope("login", title: "Passwords", symbol: "key")
                        scope("note", title: "Note", symbol: "note.text")
                        Menu {
                            ForEach(vaultKinds(), id: \.self) { kind in
                                Button { model.vaultFilter = kind } label: {
                                    Label {
                                        if model.vaultCountsAreCurrent && model.contentSync?.initialized == true {
                                            Text(vaultKindName(kind)) + Text(verbatim: " · \(model.vaultCounts.count(kind))")
                                        } else { Text(vaultKindName(kind)) }
                                    } icon: { AppGlyph.vaultItem(kind).image }
                                }
                            }
                        } label: {
                            HStack(spacing: 5) {
                                Text(moreSelected ? vaultKindName(model.vaultFilter) : "More types")
                                Image(systemName: "chevron.down").font(.footnote)
                            }.browseTabShape(selected: moreSelected, sliding: true)
                        }.id("types").accessibilityAddTraits(moreSelected ? .isSelected : []).accessibilityIdentifier("vault.types")
                    }.slidingTabCapsule().motion(Motion.move, value: "\(model.vaultFilter):\(model.vaultTag.isEmpty)")
                        .padding(.horizontal, 16)
                }
                .accessibilityIdentifier("vault.scopes").edgeFade()
                .onChange(of: model.vaultFilter) { _, filter in
                    // Reveal an offscreen selection without recentering visible tabs.
                    proxy.scrollTo(["all", "favorites", "login", "note"].contains(filter) ? filter : "types")
                }
            }
            Button { filtering = true } label: {
                Image(systemName: model.vaultTag.isEmpty ? "line.3.horizontal.decrease" : "line.3.horizontal.decrease.circle.fill").frame(width: 44, height: 44)
                if !model.vaultTag.isEmpty { Text(model.vaultTag).lineLimit(1).frame(maxWidth: 92) }
            }.accessibilityLabel("Filters").accessibilityValue(model.vaultTag).accessibilityIdentifier("vault.filters")
        }.font(.subheadline).padding(.bottom, 8)
    }

    private func scope(_ value: String, title: LocalizedStringKey, symbol: String) -> some View {
        BrowseTab(selected: model.vaultFilter == value && (value != "all" || model.vaultTag.isEmpty), sliding: true) {
            model.vaultFilter = value
            if value == "all" { model.vaultTag = "" }
        } label: {
            if sizeClass == .regular {
                if ["login", "note"].contains(value) {
                    AppGlyph.vaultItem(value).image.accessibilityHidden(true)
                } else { Image(systemName: symbol).accessibilityHidden(true) }
            }
            Text(title)
        }.id(value).accessibilityIdentifier("vault.scope.\(value)")
    }
}

private struct VaultFiltersView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        @Bindable var model = model
        Form {
            Picker("Type", selection: $model.vaultFilter) {
                Text("All").tag("all"); Text("Favorites").tag("favorites")
                ForEach(vaultKinds(), id: \.self) { Text(vaultKindName($0)).tag($0) }
            }
            Picker("Sort", selection: $model.vaultSort) {
                Text("Best match").tag("relevance")
                Text("Recently updated").tag("updated")
                Text("Name").tag("title")
            }.pickerStyle(.inline)
            Picker("Tags", selection: $model.vaultTag) {
                Text("All").tag("")
                ForEach(model.vaultCounts.tags, id: \.self) { Text($0).tag($0) }
                if !model.vaultTag.isEmpty && !model.vaultCounts.tags.contains(model.vaultTag) { Text(model.vaultTag).tag(model.vaultTag) }
            }.pickerStyle(.inline)
            Button("Clear filters") { model.vaultTag = ""; model.vaultFilter = "all" }
        }.privateScreen().navigationTitle("Filters")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
    }
}

struct VaultRow: View {
    let item: VaultItem
    var searchMeta: VaultSearchMeta? = nil
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    var body: some View {
        HStack(spacing: 10) {
            ObjectIcon(glyph: .vaultItem(item.kind))
            VStack(alignment: .leading, spacing: 2) {
                Text(highlightedText(item.title, spans: searchMeta?.titleSpans))
                    .font(.body.weight(.medium)).lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 1)
                let hint = vaultMatchHint(matchField: searchMeta?.matchField, matchValue: searchMeta?.matchValue, subtitle: item.subtitle)
                if item.subtitle.isEmpty {
                    if let hint {
                        (Text(vaultKindName(item.kind)) + Text(verbatim: " · " + hint))
                            .rowSubtitle().lineLimit(dynamicTypeSize.isAccessibilitySize ? 2 : 1)
                    } else {
                        Text(vaultKindName(item.kind)).rowSubtitle()
                    }
                } else if let hint {
                    Text(highlightedText(item.subtitle, spans: searchMeta?.subtitleSpans) + AttributedString(" · " + hint))
                        .rowSubtitle().lineLimit(dynamicTypeSize.isAccessibilitySize ? 2 : 1)
                } else {
                    Text(highlightedText(item.subtitle, spans: searchMeta?.subtitleSpans))
                        .rowSubtitle().lineLimit(dynamicTypeSize.isAccessibilitySize ? 2 : 1)
                }
            }.frame(maxWidth: .infinity, alignment: .leading)
            if item.favorite { Image(systemName: "star.fill").font(.footnote).foregroundStyle(Theme.forest).accessibilityLabel("Favorite") }
            if item.conflict { Image(systemName: "arrow.triangle.branch").accessibilityLabel("Sync conflicts") }
            else if !item.synced && item.saveState != "local_only" { Image(systemName: "arrow.triangle.2.circlepath").font(.footnote).foregroundStyle(.secondary).accessibilityLabel("Waiting to sync") }
        }
    }
}

struct VaultEditorTarget {
    let item: VaultItem?
    var kind = "login"
    var favorite = false
    var tag = ""
}
