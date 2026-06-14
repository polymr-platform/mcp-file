export type TreeNode = {
	name: string;
	path: string;
	isDir: boolean;
	children: TreeNode[];
	expanded: boolean;
	loaded?: boolean;
	loading?: boolean;
	offset?: number;
	hasMore?: boolean;
	partial?: boolean;
};
