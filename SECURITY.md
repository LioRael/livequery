# Security policy

LiveQuery is experimental. The latest published alpha is the maintained prerelease; fixes may require upgrading. The alpha.3 development branch is not a supported release. Once a stable line exists, the latest stable minor is the supported line, and fixes may require its latest patch. API/MSRV and protocol policy is recorded in docs/stability.md.

## Reporting

Report vulnerabilities privately through [GitHub private vulnerability reporting](https://github.com/LioRael/livequery/security/advisories/new). Include the affected commit/version, platform, reproduction steps, expected trust boundary and potential impact. Do not include real application credentials or sensitive snapshots. Please avoid public issues for unpatched vulnerabilities.

There is no guaranteed response SLA or paid security support. For non-sensitive bugs and questions, use repository issues.

## Trust boundary

The agent is opt-in, local and read-only. Same-user processes and root are trusted. Only explicitly registered sources are queryable. Mark sensitive fields `#[inspect(skip)]` and review nested serde serializers separately. Never expose the socket through a network proxy without an independently designed authorization layer.

Source and serializer implementations run inside the host and are trusted code. They must return promptly and uphold budgets; a timeout cannot forcibly interrupt a Rust callback. Do not use LiveQuery in setuid programs or as a multi-tenant isolation boundary. Read [the security and consistency documentation](docs/security.md) before enabling it in an application.
