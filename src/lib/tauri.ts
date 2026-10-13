import { invoke } from "@tauri-apps/api/core";

export interface WorkSummary {
  name: string;
  path: string;
  created_at: string;
}

export interface TreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children: TreeNode[];
}

export const api = {
  getWorksRoot: () => invoke<string>("get_works_root"),
  listWorks: () => invoke<WorkSummary[]>("list_works"),
  createWork: (name: string, base?: string) =>
    invoke<WorkSummary>("create_work", { name, base: base ?? null }),
  readTree: (workPath: string) => invoke<TreeNode>("read_tree", { workPath }),
  readFile: (workPath: string, relPath: string) =>
    invoke<string>("read_file", { workPath, relPath }),
  writeFile: (workPath: string, relPath: string, content: string) =>
    invoke<void>("write_file", { workPath, relPath, content }),
  createFile: (workPath: string, relPath: string, content: string) =>
    invoke<void>("create_file", { workPath, relPath, content }),
  deleteFile: (workPath: string, relPath: string) =>
    invoke<void>("delete_file", { workPath, relPath }),
  restoreFile: (workPath: string, relPath: string) =>
    invoke<void>("restore_file", { workPath, relPath }),
  gitLog: (workPath: string, limit?: number) =>
    invoke<string[]>("git_log", { workPath, limit: limit ?? null }),
};
