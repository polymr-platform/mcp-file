<template>
	<div class="app-shell">
		<header class="toolbar">
			<div class="title-block">
				<p class="kicker">MCP Files</p>
				<h1>Browser</h1>
			</div>
			<div class="toolbar-controls">
				<label class="field">
					<span>Root</span>
					<select v-model="currentRoot" @change="handleRootChange">
						<option
							v-for="root in roots"
							:key="root.path"
							:value="root.path">{{ root.path }}</option>
					</select>
				</label>
				<div class="field search-field">
					<span>Find</span>
					<input
						v-model="query"
						type="search"
						placeholder="Search files or content"
						@input="queueSearch"/>
				</div>
				<div class="mode-toggle">
					<button
						type="button"
						:class="{ active: searchMode === 'name' }"
						@click="setSearchMode('name')">Name</button>
					<button
						type="button"
						:class="{ active: searchMode === 'content' }"
						@click="setSearchMode('content')">Content</button>
				</div>
				<button
					class="refresh"
					type="button"
					@click="refreshAll"
					:disabled="loading">Refresh</button>
			</div>
		</header>
		<div class="status-bar">
			<div class="status">
				<span v-if="loading">Loading tree...</span>
				<span v-else-if="error">{{ error }}</span>
				<span v-else>{{ treeSummary }}</span>
			</div>
			<div class="status" v-if="query">{{ searchSummary }}</div>
		</div>
		<main class="workspace">
			<section class="pane pane-tree">
				<div class="pane-header">
					<h2>Files</h2>
					<span>{{ currentRootLabel }}</span>
				</div>
				<div class="pane-body">
					<div v-if="!treeNodes.length && !loading" class="empty">
						No entries found for this root.
					</div>
					<TreeNodeItem
						v-for="node in displayNodes"
						:key="node.path"
						:node="node"
						:level="0"
						:selected-path="selectedPath"
						:filter-active="filterActive"
						:is-visible="isVisible"
						:is-matched="isMatched"
						:is-expanded="isExpanded"
						:on-toggle="toggleNode"
						:on-select="openFile"
						:on-load-more="loadMore"/>
					<div v-if="!filterActive && rootHasMore" class="tree-load-more">
						<button
							class="load-more-button"
							type="button"
							:disabled="rootLoading"
							@click="loadRootMore">{{ rootLoading ? "Loading..." : "Load more" }}</button>
					</div>
				</div>
			</section>
			<section class="pane pane-preview">
				<div class="pane-header">
					<h2>Preview</h2>
					<div class="pane-actions">
						<span>{{ selectedPath || "No file selected" }}</span>
						<button
							class="copy action-button"
							type="button"
							:disabled="!selectedPath || previewLoading"
							@click="copyToClipboard">{{ copyLabel }}</button>
						<button
							class="delete action-button danger"
							type="button"
							:disabled="!selectedPath || previewLoading"
							@click="requestDelete">Delete</button>
					</div>
				</div>
				<div class="pane-body">
					<div v-if="!selectedPath" class="empty">
						Select a file to view highlighted content.
					</div>
					<div v-else-if="previewError" class="empty">{{ previewError }}</div>
					<div v-else-if="previewLoading" class="empty">
						Loading file...
					</div>
					<div
						v-else
						class="preview"
						v-html="previewHtml"></div>
				</div>
			</section>
		</main>
		<div
			v-if="deleteDialogOpen"
			class="dialog-backdrop"
			@click.self="closeDeleteDialog">
			<div
				class="dialog"
				role="dialog"
				aria-modal="true"
				aria-labelledby="delete-title">
				<h3 id="delete-title" class="dialog-title">Delete file?</h3>
				<p class="dialog-body">
					This removes
					<strong>{{ deleteTargetPath }}</strong>
					from
					<strong>{{ currentRoot }}</strong>
					.
					This action cannot be undone.
				</p>
				<p v-if="deleteError" class="dialog-error">{{ deleteError }}</p>
				<div class="dialog-actions">
					<button
						class="action-button"
						type="button"
						:disabled="deleteBusy"
						@click="closeDeleteDialog">Cancel</button>
					<button
						class="action-button danger"
						type="button"
						:disabled="deleteBusy"
						@click="confirmDelete">Delete</button>
				</div>
			</div>
		</div>
	</div>
</template>
<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import TreeNodeItem from "./TreeNodeItem.vue";
import type { TreeNode } from "./types";
type RootItem = { path: string; default?: boolean };
type SearchMode = "name" | "content";
const roots = ref<RootItem[]>([]);
const currentRoot = ref(".");
const treeNodes = ref<TreeNode[]>([]);
const searchNodes = ref<TreeNode[]>([]);
const selectedPath = ref("");
const previewHtml = ref("");
const loading = ref(false);
const rootLoading = ref(false);
const previewLoading = ref(false);
const error = ref("");
const previewError = ref("");
const copyLabel = ref("Copy");
const deleteDialogOpen = ref(false);
const deleteTargetPath = ref("");
const deleteBusy = ref(false);
const deleteError = ref("");
const query = ref("");
const searchMode = ref<SearchMode>("name");
const matchPaths = ref<Set<string>>(new Set());
const rootOffset = ref(0);
const rootHasMore = ref(false);
const PAGE_SIZE = 500;
let mcpApp: { connect?: () => Promise<unknown>; callServerTool?: (params: unknown) => Promise<unknown>; ontoolresult?: (result: unknown) => void } | null = null;
let searchTimer: number | undefined;
const filterActive = computed(() => query.value.trim().length > 0);
const displayNodes = computed(() => filterActive.value ? searchNodes.value : treeNodes.value);
const currentRootLabel = computed(() => {
	if (!currentRoot.value) {
		return "";
	}
	const rootEntry = roots.value.find((root) => root.path === currentRoot.value);
	if (!rootEntry) {
		return currentRoot.value;
	}
	return rootEntry.default ? `${rootEntry.path} (default)` : rootEntry.path;
});
const treeSummary = computed(() => {
	const count = countNodes(displayNodes.value);
	return `${count.files} file${count.files === 1 ? "" : "s"}, ${count.dirs} folder${
		count.dirs === 1 ? "" : "s"
	}`;
});
const searchSummary = computed(() => {
	if (!filterActive.value) {
		return "";
	}
	const matches = matchPaths.value.size;
	const label = searchMode.value === "content" ? "content" : "name";
	return `${matches} match${matches === 1 ? "" : "es"} (${label})`;
});
function ensureMcp() {
	if (mcpApp?.callServerTool) {
		return mcpApp.callServerTool.bind(mcpApp);
	}
	const mcp = (window as unknown as { mcp?: { callTool?: any } }).mcp;
	if (!mcp?.callTool) {
		throw new Error("mcp.callTool unavailable");
	}
	return mcp.callTool.bind(mcp);
}
async function connectMcpApp() {
	const AppCtor = (window as unknown as { App?: new() => any }).App;
	if (!AppCtor) {
		return;
	}
	const app = new AppCtor();
	app.ontoolresult = (result: unknown) => {
		console.debug("[mcp-app] tool result notification", result);
	};
	await app.connect();
	mcpApp = app;
}
async function callTool(name: string, argumentsParam: Record<string, unknown> = {}, meta?: Record<string, unknown>) {
	const call = ensureMcp();
	const payload: { name: string; arguments?: Record<string, unknown>; _meta?: Record<string, unknown> } = { name, arguments: argumentsParam };
	if (meta) {
		payload._meta = meta;
	}
	console.debug("[mcp-ui] tool call", payload);
	const result = await call(payload);
	console.debug("[mcp-ui] tool result", name, result);
	return result;
}
function resolvePath(root: string, rel: string) {
	if (!rel) {
		return root;
	}
	if (rel.startsWith("/")) {
		return rel;
	}
	if (root === "." || root === "") {
		return rel;
	}
	if (root.endsWith("/")) {
		return `${root}${rel}`;
	}
	return `${root}/${rel}`;
}
function normalizeEntry(entry: string) {
	if (entry.endsWith("/")) {
		return { path: entry.slice(0, -1), isDir: true };
	}
	return { path: entry, isDir: false };
}
function joinPath(base: string, child: string) {
	if (!base) {
		return child;
	}
	return `${base}/${child}`;
}
function entryName(path: string) {
	const parts = path.split("/");
	return parts[parts.length - 1] || path;
}
function sortTree(nodes: TreeNode[]) {
	nodes.sort((a, b) => {
			if (a.isDir && !b.isDir) {
				return -1;
			}
			if (!a.isDir && b.isDir) {
				return 1;
			}
			return a.name.localeCompare(b.name);
		});
	nodes.forEach((node) => {
			if (node.children.length) {
				sortTree(node.children);
			}
		});
}
function createNode(path: string, isDir: boolean, partial = false): TreeNode {
	return {
		name: entryName(path),
		path,
		isDir,
		children: [],
		expanded: false,
		loaded: !isDir,
		loading: false,
		offset: 0,
		hasMore: false,
		partial
	};
}
function buildSearchTree(matches: string[]) {
	const nodes = new Map<string, TreeNode>();
	const rootsOut: TreeNode[] = [];
	const getNode = (path: string, isDir: boolean, partial: boolean) => {
		const existing = nodes.get(path);
		if (existing) {
			if (isDir && !existing.isDir) {
				existing.isDir = true;
			}
			if (!partial) {
				existing.partial = false;
			}
			return existing;
		}
		const node = createNode(path, isDir, partial);
		nodes.set(path, node);
		return node;
	};
	matches.map(normalizeEntry)
		.forEach(({ path, isDir }) => {
			if (!path) {
				return;
			}
			const parts = path.split("/");
			let currentPath = "";
			let parentNode: TreeNode | null = null;
			parts.forEach((segment, index) => {
					currentPath = currentPath ? `${currentPath}/${segment}` : segment;
					const isLeaf = index === parts.length - 1;
					const nodeIsDir = isLeaf ? isDir : true;
					const node = getNode(currentPath, nodeIsDir, !isLeaf);
					if (parentNode && !parentNode.children.includes(node)) {
						parentNode.children.push(node);
					}
					if (!parentNode && !rootsOut.includes(node)) {
						rootsOut.push(node);
					}
					parentNode = node;
				});
		});
	const expandAll = (node: TreeNode) => {
		if (node.isDir && node.children.length) {
			node.expanded = true;
		}
		node.children.forEach(expandAll);
	};
	rootsOut.forEach(expandAll);
	sortTree(rootsOut);
	searchNodes.value = rootsOut;
}
function countNodes(nodes: TreeNode[]) {
	let files = 0;
	let dirs = 0;
	const walk = (node: TreeNode) => {
		if (node.isDir) {
			dirs += 1;
		}
		else {
			files += 1;
		}
		node.children.forEach(walk);
	};
	nodes.forEach(walk);
	return { files, dirs };
}
function isVisible(node: TreeNode) {
	return true;
}
function isMatched(node: TreeNode) {
	return matchPaths.value.has(node.path);
}
function isExpanded(node: TreeNode) {
	return node.expanded;
}
function toggleNode(node: TreeNode) {
	if (!node.isDir) {
		return;
	}
	node.expanded = !node.expanded;
	if (!filterActive.value && node.expanded && !node.loaded) {
		loadChildren(node, false);
	}
}
async function listDirectory(path: string, offset: number) {
	const root = path ? resolvePath(currentRoot.value, path) : currentRoot.value;
	const result = await callTool("find_files", {
		root,
		pattern: "",
		max_depth: 1,
		limit: PAGE_SIZE,
		offset
	});
	const structured = result?.structuredContent ?? result;
	const matches = (structured?.matches || []) as string[];
	const truncated = Boolean(structured?.truncated);
	return { matches, truncated };
}
async function loadChildren(node: TreeNode, append: boolean) {
	if (node.loading) {
		return;
	}
	node.loading = true;
	try {
		const offset = append ? node.offset ?? 0 : 0;
		const { matches, truncated } = await listDirectory(node.path, offset);
		if (!append) {
			node.children = [];
			node.offset = 0;
		}
		const existing = new Map(node.children.map((child) => [child.path, child]));
		matches.map(normalizeEntry)
			.forEach(({ path, isDir }) => {
				if (!path) {
					return;
				}
				const childPath = joinPath(node.path, path);
				if (existing.has(childPath)) {
					return;
				}
				const child = createNode(childPath, isDir, false);
				node.children.push(child);
			});
		sortTree(node.children);
		node.offset = (node.offset ?? 0) + matches.length;
		node.hasMore = truncated;
		node.loaded = true;
		node.partial = false;
	}
	catch (err) {
		error.value = errinstanceofError ? err.message : "Failed to load directory";
	}
	finally {
		node.loading = false;
	}
}
function loadMore(node: TreeNode) {
	if (!node.isDir || filterActive.value) {
		return;
	}
	loadChildren(node, true);
}
async function loadRootMore() {
	if (rootLoading.value) {
		return;
	}
	rootLoading.value = true;
	try {
		const { matches, truncated } = await listDirectory("", rootOffset.value);
		matches.map(normalizeEntry)
			.forEach(({ path, isDir }) => {
				if (!path) {
					return;
				}
				const exists = treeNodes.value.some((node) => node.path === path);
				if (exists) {
					return;
				}
				treeNodes.value.push(createNode(path, isDir, false));
			});
		sortTree(treeNodes.value);
		rootOffset.value += matches.length;
		rootHasMore.value = truncated;
	}
	catch (err) {
		error.value = errinstanceofError ? err.message : "Failed to load directory";
	}
	finally {
		rootLoading.value = false;
	}
}
async function openFile(node: TreeNode) {
	if (node.isDir) {
		return;
	}
	previewLoading.value = true;
	previewError.value = "";
	selectedPath.value = node.path;
	copyLabel.value = "Copy";
	try {
		const absolutePath = resolvePath(currentRoot.value, node.path);
		const result = await callTool("read_file", { path: absolutePath, limit: 0 }, { highlight: true });
		const structured = result?.structuredContent ?? result;
		previewHtml.value = structured?.content || "";
	}
	catch (err) {
		previewError.value = errinstanceofError ? err.message : "Failed to load file";
	}
	finally {
		previewLoading.value = false;
	}
}
function stripLineNumbers(content: string) {
	return content;
}
async function copyToClipboard() {
	if (!selectedPath.value) {
		return;
	}
	copyLabel.value = "Copying...";
	try {
		const absolutePath = resolvePath(currentRoot.value, selectedPath.value);
		const result = await callTool("read_file", { path: absolutePath, limit: 0 });
		const structured = result?.structuredContent ?? result;
		const raw = structured?.content ? String(structured.content) : "";
		const cleaned = stripLineNumbers(raw);
		if (navigator.clipboard?.writeText) {
			await navigator.clipboard.writeText(cleaned);
		}
		else {
			const textarea = document.createElement("textarea");
			textarea.value = cleaned;
			textarea.setAttribute("readonly", "true");
			textarea.style.position = "absolute";
			textarea.style.left = "-9999px";
			document.body.appendChild(textarea);
			textarea.select();
			document.execCommand("copy");
			textarea.remove();
		}
		copyLabel.value = "Copied";
		window.setTimeout(
			() => {
				copyLabel.value = "Copy";
			},
			1200
		);
	}
	catch (err) {
		copyLabel.value = "Copy";
		previewError.value = errinstanceofError ? err.message : "Copy failed";
	}
}
function requestDelete() {
	if (!selectedPath.value) {
		return;
	}
	deleteTargetPath.value = selectedPath.value;
	deleteError.value = "";
	deleteDialogOpen.value = true;
}
function closeDeleteDialog() {
	if (deleteBusy.value) {
		return;
	}
	deleteDialogOpen.value = false;
	deleteError.value = "";
}
async function confirmDelete() {
	if (!deleteTargetPath.value) {
		return;
	}
	deleteBusy.value = true;
	deleteError.value = "";
	try {
		const absolutePath = resolvePath(currentRoot.value, deleteTargetPath.value);
		await callTool("delete_file", { paths: [absolutePath] });
		if (selectedPath.value === deleteTargetPath.value) {
			selectedPath.value = "";
			previewHtml.value = "";
			previewError.value = "";
			copyLabel.value = "Copy";
		}
		deleteDialogOpen.value = false;
		await loadTree();
		if (query.value.trim()) {
			await runSearch();
		}
	}
	catch (err) {
		deleteError.value = errinstanceofError ? err.message : "Delete failed";
	}
	finally {
		deleteBusy.value = false;
	}
}
async function loadRoots() {
	const result = await callTool("list_roots");
	const structured = result?.structuredContent ?? result;
	console.debug("[mcp-ui] list_roots structured", structured);
	const items = (structured?.roots || []) as RootItem[];
	roots.value = items;
	const defaultRoot = items.find((root) => root.default)?.path;
	currentRoot.value = defaultRoot || items[0]?.path || ".";
}
async function loadTree() {
	if (!currentRoot.value) {
		return;
	}
	loading.value = true;
	error.value = "";
	try {
		const { matches, truncated } = await listDirectory("", 0);
		console.debug("[mcp-ui] find_files structured", { matches: matches.length, truncated });
		treeNodes.value = matches.map(normalizeEntry)
			.filter((entry) => entry.path)
			.map((entry) => createNode(entry.path, entry.isDir, false));
		sortTree(treeNodes.value);
		rootOffset.value = matches.length;
		rootHasMore.value = truncated;
	}
	catch (err) {
		error.value = errinstanceofError ? err.message : "Failed to load tree";
		treeNodes.value = [];
		rootOffset.value = 0;
		rootHasMore.value = false;
	}
	finally {
		loading.value = false;
	}
}
async function runSearch() {
	const term = query.value.trim();
	if (!term) {
		matchPaths.value = new Set();
		searchNodes.value = [];
		return;
	}
	try {
		if (searchMode.value === "name") {
			const result = await callTool("find_files", {
				root: currentRoot.value,
				pattern: term,
				glob: true,
				limit: 0
			});
			const structured = result?.structuredContent ?? result;
			const entries = (structured?.matches || []) as string[];
			const normalized = entries.map((entry) => normalizeEntry(entry).path);
			matchPaths.value = new Set(normalized);
			buildSearchTree(entries);
		}
		else {
			const result = await callTool("search_files", { root: currentRoot.value, pattern: term, context: 0 });
			const structured = result?.structuredContent ?? result;
			const files = (structured?.files || []) as Array<{ path: string }>;
			const paths = files.map((file) => file.path);
			matchPaths.value = new Set(paths);
			buildSearchTree(paths);
		}
	}
	catch (err) {
		error.value = errinstanceofError ? err.message : "Search failed";
		matchPaths.value = new Set();
		searchNodes.value = [];
	}
}
function queueSearch() {
	if (searchTimer) {
		window.clearTimeout(searchTimer);
	}
	searchTimer = window.setTimeout(
		() => {
			runSearch();
		},
		250
	);
}
function setSearchMode(mode: SearchMode) {
	if (searchMode.value === mode) {
		return;
	}
	searchMode.value = mode;
	if (query.value.trim()) {
		runSearch();
	}
}
async function refreshAll() {
	await loadTree();
	if (query.value.trim()) {
		await runSearch();
	}
}
async function handleRootChange() {
	selectedPath.value = "";
	previewHtml.value = "";
	matchPaths.value = new Set();
	searchNodes.value = [];
	query.value = "";
	await loadTree();
}
onMounted(async() => {
	try {
		await connectMcpApp();
		await loadRoots();
		await loadTree();
	}
	catch (err) {
		error.value = errinstanceofError ? err.message : "Failed to initialize";
	}
});
</script>
