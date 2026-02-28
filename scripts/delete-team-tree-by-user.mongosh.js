#!/usr/bin/env mongosh

/*
 * Delete full team trees associated with a user, including all users in
 * those teams and related data.
 *
 * Usage examples:
 *   mongosh conman --eval 'globalThis.DELETE_TEAM_TREE={email:"you@example.com"}' scripts/delete-team-tree-by-user.mongosh.js
 *   mongosh conman --eval 'globalThis.DELETE_TEAM_TREE={email:"you@example.com",execute:true}' scripts/delete-team-tree-by-user.mongosh.js
 *   DELETE_TEAM_TREE_EMAIL=you@example.com DELETE_TEAM_TREE_EXECUTE=1 mongosh conman scripts/delete-team-tree-by-user.mongosh.js
 *
 * Flags:
 *   --email <email>               Seed user email (can be repeated or comma-separated)
 *   --user-id <oid>               Seed user id(s) (repeat/comma-separated)
 *   --team-id <oid>               Team id(s) to delete directly (repeat/comma-separated)
 *   --allow-shared-users          Allow deleting users that are members of non-target teams
 *   --execute                     Actually run deletes/updates (default is dry-run)
 *   --db <name>                   Use sibling DB name instead of current one
 *   --help                        Print usage
 */

(function main() {
  const cliArgs = parseArgsSafe(process.argv.slice(2));
  const envArgs = parseEnvArgs();
  const globalArgs = parseGlobalArgs();
  const args = { ...envArgs, ...cliArgs, ...globalArgs };

  if (args.help) {
    printUsage();
    quit(0);
  }

  const execute = Boolean(args.execute);
  const allowSharedUsers = Boolean(args["allow-shared-users"]);
  const database = args.db ? db.getSiblingDB(String(args.db)) : db;

  const seedEmails = listArg(args.email).map((v) => String(v).trim().toLowerCase()).filter(Boolean);
  const seedUserIds = listArg(args["user-id"]).map(parseObjectIdSafe).filter(Boolean);
  const explicitTeamIds = listArg(args["team-id"]).map(parseObjectIdSafe).filter(Boolean);

  if (!seedEmails.length && !seedUserIds.length && !explicitTeamIds.length) {
    print("Missing target. Provide at least one of `--email`, `--user-id`, or `--team-id`.");
    printUsage();
    quit(1);
  }

  const seedUsersByEmail = seedEmails.length
    ? database.users.find({ email: { $in: seedEmails } }, { _id: 1, email: 1, name: 1 }).toArray()
    : [];

  const unknownEmails = seedEmails.filter(
    (email) => !seedUsersByEmail.some((u) => String(u.email).toLowerCase() === email),
  );
  if (unknownEmails.length) {
    print("These emails were not found: " + unknownEmails.join(", "));
    quit(1);
  }

  const allSeedUserIds = uniqueObjectIds([
    ...seedUserIds,
    ...seedUsersByEmail.map((u) => u._id),
  ]);

  let targetTeamIds = uniqueObjectIds(explicitTeamIds);
  if (allSeedUserIds.length) {
    const memberships = database.team_memberships
      .find({ user_id: { $in: allSeedUserIds } }, { team_id: 1 })
      .toArray();
    targetTeamIds = uniqueObjectIds([
      ...targetTeamIds,
      ...memberships.map((m) => m.team_id),
    ]);
  }

  if (!targetTeamIds.length) {
    print("No teams found for the provided user/team input.");
    quit(1);
  }

  const targetTeams = database.teams
    .find({ _id: { $in: targetTeamIds } }, { _id: 1, name: 1, slug: 1 })
    .toArray();

  const teamMemberships = database.team_memberships
    .find({ team_id: { $in: targetTeamIds } }, { _id: 1, team_id: 1, user_id: 1, role: 1 })
    .toArray();

  const teamUserIds = uniqueObjectIds(teamMemberships.map((m) => m.user_id));
  if (!teamUserIds.length) {
    print("Target teams have no members. Aborting.");
    quit(1);
  }

  const teamUsers = database.users
    .find({ _id: { $in: teamUserIds } }, { _id: 1, email: 1, name: 1 })
    .toArray();

  const sharedUserRows = database.team_memberships
    .aggregate([
      {
        $match: {
          user_id: { $in: teamUserIds },
          team_id: { $nin: targetTeamIds },
        },
      },
      {
        $group: {
          _id: "$user_id",
          other_team_ids: { $addToSet: "$team_id" },
          count: { $sum: 1 },
        },
      },
    ])
    .toArray();

  if (sharedUserRows.length && !allowSharedUsers) {
    print("Safety stop: some users are also members of non-target teams.");
    print("Re-run with `--allow-shared-users` if you want to delete them anyway.");
    for (const row of sharedUserRows) {
      const user = teamUsers.find((u) => objectIdHex(u._id) === objectIdHex(row._id));
      const label = user ? `${user.email} (${objectIdHex(row._id)})` : objectIdHex(row._id);
      print(`  - ${label} has ${row.count} membership(s) outside target teams`);
    }
    quit(1);
  }

  const repos = database.repos
    .find({ team_id: { $in: targetTeamIds } }, { _id: 1, team_id: 1, name: 1, repo_path: 1 })
    .toArray();
  const repoIds = uniqueObjectIds(repos.map((r) => r._id));

  const changesetDeleteFilter = orFilter([
    inFilter("repo_id", repoIds),
    inFilter("author_user_id", teamUserIds),
  ]);
  const changesetIds = changesetDeleteFilter
    ? uniqueObjectIds(
        database.changesets.find(changesetDeleteFilter, { _id: 1 }).toArray().map((c) => c._id),
      )
    : [];

  const jobDeleteFilter = orFilter([
    inFilter("repo_id", repoIds),
    inFilter("created_by", teamUserIds),
  ]);
  const jobIds = jobDeleteFilter
    ? uniqueObjectIds(database.jobs.find(jobDeleteFilter, { _id: 1 }).toArray().map((j) => j._id))
    : [];

  print("Database: " + database.getName());
  print("Mode: " + (execute ? "EXECUTE" : "DRY-RUN"));
  print("");
  print("Target teams:");
  for (const t of targetTeams) {
    print(`  - ${t.name} [${t.slug}] (${objectIdHex(t._id)})`);
  }
  print("Target users:");
  for (const u of teamUsers) {
    print(`  - ${u.email} (${objectIdHex(u._id)})`);
  }
  print(`Target repos: ${repoIds.length}`);
  print(`Target changesets: ${changesetIds.length}`);
  print(`Target jobs: ${jobIds.length}`);
  print("");

  const ops = [
    deleteOp("changeset_revisions by changeset", "changeset_revisions", inFilter("changeset_id", changesetIds)),
    deleteOp("changeset_profile_overrides by changeset", "changeset_profile_overrides", inFilter("changeset_id", changesetIds)),
    deleteOp(
      "changeset_comments by repo/author/changeset",
      "changeset_comments",
      orFilter([
        inFilter("repo_id", repoIds),
        inFilter("author_user_id", teamUserIds),
        inFilter("changeset_id", changesetIds),
      ]),
    ),
    deleteOp("changesets by repo/author", "changesets", changesetDeleteFilter),
    updateOp(
      "changesets pull approvals from deleted users",
      "changesets",
      inFilter("approvals.user_id", teamUserIds.map(objectIdHex)),
      { $pull: { approvals: { user_id: { $in: teamUserIds.map(objectIdHex) } } } },
    ),
    deleteOp("deployments by repo/creator", "deployments", orFilter([inFilter("repo_id", repoIds), inFilter("created_by", teamUserIds)])),
    updateOp(
      "deployments pull approvers from deleted users",
      "deployments",
      inFilter("approvals", teamUserIds),
      { $pull: { approvals: { $in: teamUserIds } } },
    ),
    deleteOp("release_batches by repo", "release_batches", inFilter("repo_id", repoIds)),
    deleteOp("job_logs by repo/job", "job_logs", orFilter([inFilter("repo_id", repoIds), inFilter("job_id", jobIds)])),
    deleteOp("jobs by repo/creator", "jobs", jobDeleteFilter),
    deleteOp("temp_environments by repo/owner", "temp_environments", orFilter([inFilter("repo_id", repoIds), inFilter("owner_user_id", teamUserIds)])),
    deleteOp("workspaces by repo/owner", "workspaces", orFilter([inFilter("repo_id", repoIds), inFilter("owner_user_id", teamUserIds)])),
    deleteOp("apps by repo", "apps", inFilter("repo_id", repoIds)),
    deleteOp("environments by repo", "environments", inFilter("repo_id", repoIds)),
    deleteOp("runtime_profile_revisions by repo", "runtime_profile_revisions", inFilter("repo_id", repoIds)),
    deleteOp("runtime_profiles by repo", "runtime_profiles", inFilter("repo_id", repoIds)),
    deleteOp("ui_config by repo/configurer", "ui_config", orFilter([inFilter("repo_id", repoIds), inFilter("configured_by", teamUserIds)])),
    deleteOp("repo_memberships by repo/user", "repo_memberships", orFilter([inFilter("repo_id", repoIds), inFilter("user_id", teamUserIds)])),
    deleteOp("repos by team", "repos", inFilter("team_id", targetTeamIds)),
    deleteOp("invites by team/inviter", "invites", orFilter([inFilter("team_id", targetTeamIds), inFilter("invited_by", teamUserIds)])),
    deleteOp("team_memberships by team/user", "team_memberships", orFilter([inFilter("team_id", targetTeamIds), inFilter("user_id", teamUserIds)])),
    deleteOp("teams", "teams", inFilter("_id", targetTeamIds)),
    deleteOp("notification_events by repo/user", "notification_events", orFilter([inFilter("repo_id", repoIds), inFilter("user_id", teamUserIds)])),
    deleteOp("notification_preferences by user", "notification_preferences", inFilter("user_id", teamUserIds)),
    deleteOp("password_reset_tokens by user", "password_reset_tokens", inFilter("user_id", teamUserIds)),
    deleteOp("audit_events by repo/actor", "audit_events", orFilter([inFilter("repo_id", repoIds), inFilter("actor_user_id", teamUserIds)])),
    deleteOp("users", "users", inFilter("_id", teamUserIds)),
  ].filter((op) => Boolean(op.filter));

  print("Planned operations:");
  for (const op of ops) {
    const coll = database.getCollection(op.collection);
    const matched = coll.countDocuments(op.filter);
    if (op.type === "delete") {
      print(`  - ${op.name}: delete ${matched} from ${op.collection}`);
    } else {
      print(`  - ${op.name}: update ${matched} in ${op.collection}`);
    }
  }

  if (!execute) {
    print("");
    print("Dry-run only. Re-run with `--execute` to apply changes.");
    quit(0);
  }

  print("");
  print("Executing operations...");
  for (const op of ops) {
    const coll = database.getCollection(op.collection);
    if (op.type === "delete") {
      const res = coll.deleteMany(op.filter);
      print(`  - ${op.name}: deleted ${res.deletedCount}`);
      continue;
    }

    const res = coll.updateMany(op.filter, op.update);
    print(`  - ${op.name}: matched ${res.matchedCount}, modified ${res.modifiedCount}`);
  }

  print("");
  print("Done.");
})();

function parseArgs(argv) {
  const out = {};
  const boolKeys = new Set(["execute", "allow-shared-users", "help"]);
  const valueKeys = new Set(["email", "user-id", "team-id", "db"]);
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (!token.startsWith("--")) continue;

    const key = token.slice(2);
    if (!boolKeys.has(key) && !valueKeys.has(key)) {
      continue;
    }

    if (boolKeys.has(key)) {
      out[key] = true;
      continue;
    }

    const value = argv[i + 1];
    if (value == null || value.startsWith("--")) {
      throw new Error(`Missing value for --${key}`);
    }
    i += 1;

    if (out[key] == null) {
      out[key] = value;
    } else if (Array.isArray(out[key])) {
      out[key].push(value);
    } else {
      out[key] = [out[key], value];
    }
  }
  return out;
}

function parseArgsSafe(argv) {
  try {
    return parseArgs(argv);
  } catch (err) {
    print(String(err));
    printUsage();
    quit(1);
  }
}

function parseGlobalArgs() {
  if (typeof globalThis.DELETE_TEAM_TREE === "object" && globalThis.DELETE_TEAM_TREE !== null) {
    return globalThis.DELETE_TEAM_TREE;
  }
  if (typeof globalThis.cleanupInput === "object" && globalThis.cleanupInput !== null) {
    return globalThis.cleanupInput;
  }
  return {};
}

function parseEnvArgs() {
  const out = {};
  if (typeof process !== "undefined" && process && process.env) {
    if (process.env.DELETE_TEAM_TREE_EMAIL) out.email = process.env.DELETE_TEAM_TREE_EMAIL;
    if (process.env.DELETE_TEAM_TREE_USER_ID) out["user-id"] = process.env.DELETE_TEAM_TREE_USER_ID;
    if (process.env.DELETE_TEAM_TREE_TEAM_ID) out["team-id"] = process.env.DELETE_TEAM_TREE_TEAM_ID;
    if (process.env.DELETE_TEAM_TREE_DB) out.db = process.env.DELETE_TEAM_TREE_DB;
    if (truthyEnv(process.env.DELETE_TEAM_TREE_ALLOW_SHARED_USERS)) {
      out["allow-shared-users"] = true;
    }
    if (truthyEnv(process.env.DELETE_TEAM_TREE_EXECUTE)) {
      out.execute = true;
    }
    if (truthyEnv(process.env.DELETE_TEAM_TREE_HELP)) {
      out.help = true;
    }
  }
  return out;
}

function truthyEnv(value) {
  if (value == null) return false;
  const normalized = String(value).trim().toLowerCase();
  return normalized === "1" || normalized === "true" || normalized === "yes" || normalized === "y";
}

function listArg(value) {
  if (value == null) return [];
  const arr = Array.isArray(value) ? value : [value];
  return arr
    .flatMap((v) => String(v).split(","))
    .map((v) => v.trim())
    .filter(Boolean);
}

function parseObjectIdSafe(value) {
  try {
    return ObjectId(String(value));
  } catch (_err) {
    print(`Invalid ObjectId ignored: ${value}`);
    return null;
  }
}

function objectIdHex(id) {
  if (!id) return "";
  if (typeof id === "string") return id;
  return id.valueOf();
}

function uniqueObjectIds(ids) {
  const map = new Map();
  for (const id of ids) {
    if (!id) continue;
    const oid = typeof id === "string" ? parseObjectIdSafe(id) : id;
    if (!oid) continue;
    map.set(objectIdHex(oid), oid);
  }
  return Array.from(map.values());
}

function inFilter(field, values) {
  if (!values || values.length === 0) return null;
  return { [field]: { $in: values } };
}

function orFilter(parts) {
  const clean = (parts || []).filter(Boolean);
  if (clean.length === 0) return null;
  if (clean.length === 1) return clean[0];
  return { $or: clean };
}

function deleteOp(name, collection, filter) {
  return { type: "delete", name, collection, filter };
}

function updateOp(name, collection, filter, update) {
  return { type: "update", name, collection, filter, update };
}

function printUsage() {
  print("Usage:");
  print('  mongosh <db> --eval \'globalThis.DELETE_TEAM_TREE={email:\"user@example.com\"}\' scripts/delete-team-tree-by-user.mongosh.js');
  print('  mongosh <db> --eval \'globalThis.DELETE_TEAM_TREE={\"user-id\":\"<oid>\",execute:true}\' scripts/delete-team-tree-by-user.mongosh.js');
  print("  DELETE_TEAM_TREE_EMAIL=user@example.com DELETE_TEAM_TREE_EXECUTE=1 mongosh <db> scripts/delete-team-tree-by-user.mongosh.js");
  print("");
  print("Config keys:");
  print("  --email <email>           Seed user email (repeat/comma-separated)");
  print("  --user-id <oid>           Seed user id (repeat/comma-separated)");
  print("  --team-id <oid>           Directly target team id(s)");
  print("  --allow-shared-users      Allow deleting users in other teams too");
  print("  --execute                 Apply changes (default: dry-run)");
  print("  --db <name>               Use sibling db");
  print("  --help                    Show help");
  print("");
  print("Equivalent env vars:");
  print("  DELETE_TEAM_TREE_EMAIL, DELETE_TEAM_TREE_USER_ID, DELETE_TEAM_TREE_TEAM_ID");
  print("  DELETE_TEAM_TREE_ALLOW_SHARED_USERS, DELETE_TEAM_TREE_EXECUTE, DELETE_TEAM_TREE_DB");
}
