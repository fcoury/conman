export interface Workspace {
  id: string;
  repo_id: string;
  owner_user_id: string;
  branch_name: string;
  title: string | null;
  is_default: boolean;
  base_ref_type: string;
  base_ref_value: string;
  head_sha: string;
  created_at: string;
  updated_at: string;
}

export interface FileEntry {
  path: string;
  entry_type: 'file' | 'dir';
  size: number;
  oid: string;
}

export interface FileTreeResponse {
  path: string;
  entries: FileEntry[];
}

export interface FileContentResponse {
  path: string;
  content: string; // base64
  size: number;
}

export interface FileWriteResponse {
  commit_sha: string;
  path: string;
}

// Client-side tree structure built from the flat FileEntry list
export interface TreeNode {
  name: string;
  path: string;
  type: 'file' | 'dir';
  children?: TreeNode[];
}
