import type { Role } from "@/types/api";

const ROLE_RANK: Record<Role, number> = {
  member: 1,
  reviewer: 2,
  config_manager: 3,
  admin: 4,
  owner: 5,
};

export function hasMinimumRole(role: Role | null | undefined, minimum: Role): boolean {
  if (!role) return false;
  return ROLE_RANK[role] >= ROLE_RANK[minimum];
}

export function formatRoleLabel(role: Role | null | undefined): string {
  if (!role) return "No role";
  if (role === "config_manager") return "Config Manager";
  return role.charAt(0).toUpperCase() + role.slice(1);
}

export function canReviewChangesets(role: Role | null | undefined): boolean {
  return hasMinimumRole(role, "reviewer");
}

export function canManageReleases(role: Role | null | undefined): boolean {
  return hasMinimumRole(role, "config_manager");
}

export function canManageAdministration(role: Role | null | undefined): boolean {
  return hasMinimumRole(role, "admin");
}
