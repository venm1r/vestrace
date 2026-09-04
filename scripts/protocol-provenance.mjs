export const PINNED_PROTOCOL_PROVENANCE = {
  a2a: {
    commit: '3303592588e388e62e0f69f701af531d2f4e3991',
    release: 'v1.0.1',
    release_url: 'https://github.com/a2aproject/A2A/releases/tag/v1.0.1',
    repository: 'https://github.com/a2aproject/A2A.git',
    wire_header: 'A2A-Version: 1.0',
    wire_version: '1.0',
  },
  crates: {
    'a2a-client-lf': {
      adapter_owner: 'vestrace-infrastructure',
      archive_url: 'https://crates.io/api/v1/crates/a2a-client-lf/0.2.1/download',
      checksum: 'f68a06a40df172bb5ae0f25e3d49e922f0d33f8af49d0b1276307857dbd28ad2',
      repository: 'https://github.com/a2aproject/a2a-rs',
      source: 'registry+https://github.com/rust-lang/crates.io-index',
      vcs_commit: '0b19af0e2805455c01f8f2b7fb52c5d5ec1bce95',
      version: '0.2.1',
    },
    'a2a-lf': {
      adapter_owner: 'shared',
      archive_url: 'https://crates.io/api/v1/crates/a2a-lf/0.3.0/download',
      checksum: '7fb24275cca126dc3301d272eef07bd4cefd87f9a7dd5d6f27200fe87e8a83d0',
      repository: 'https://github.com/a2aproject/a2a-rs',
      source: 'registry+https://github.com/rust-lang/crates.io-index',
      vcs_commit: '73c72eeed997fddf5a00be44068575437e7f3f82',
      version: '0.3.0',
    },
    'a2a-server-lf': {
      adapter_owner: 'vestrace-http',
      archive_url: 'https://crates.io/api/v1/crates/a2a-server-lf/0.4.1/download',
      checksum: 'c4df08dff9607c4045c892b58f3824bb215262a37bce33f1ab42a72a5c9acd51',
      repository: 'https://github.com/a2aproject/a2a-rs',
      source: 'registry+https://github.com/rust-lang/crates.io-index',
      vcs_commit: '0b19af0e2805455c01f8f2b7fb52c5d5ec1bce95',
      version: '0.4.1',
    },
  },
  external_corpus: {
    aggregate_sha256: '762c25f3781f1ba30783e218db64aa4dc9536add490f61255056bfe8635d8594',
    manifest_path: 'docs/external-corpus/vestrace-docss-2026-08-19.manifest.json',
    manifest_sha256: 'f8100e1bcb070da6c6f540644d2fcae0862dd5c7cc3337cfa7c5bd29d10aac02',
  },
  frozen_spec: {
    path: 'docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md',
    sha256: 'b31b5be62504e1a65f411cd31b446cd41b3d032b7282f1aa42907706ef9c1473',
  },
  npm: {
    '@ag-ui/client': {
      attestation_url: 'https://registry.npmjs.org/-/npm/v1/attestations/@ag-ui%2fclient@0.0.58',
      integrity: 'sha512-9tAUJ6Ot0y2f5Va7xGFhUSO5OPAjilMsPdmKNmAUR774LKWcvNpriJ6mqH3p/ewUue1zfoUo4vpX+9Xo/zsuzA==',
      metadata_url: 'https://registry.npmjs.org/@ag-ui%2fclient/0.0.58',
      repository: 'git+https://github.com/ag-ui-protocol/ag-ui.git',
      resolved_dependency_uri: 'git+https://github.com/ag-ui-protocol/ag-ui@refs/heads/main',
      resolved_git_commit: '0c0b88a3fe087a631decd6225efaf40e068e4449',
      version: '0.0.58',
    },
    '@ag-ui/core': {
      attestation_url: 'https://registry.npmjs.org/-/npm/v1/attestations/@ag-ui%2fcore@0.0.58',
      integrity: 'sha512-XgGb7YmhV+yMBaEmlrpsd5S+nUxq0JgSegss2t4gIFR1j7w3w0ibtKfRgcQHWeMvwZxcT5S28VEEarqtgxYYHw==',
      metadata_url: 'https://registry.npmjs.org/@ag-ui%2fcore/0.0.58',
      repository: 'git+https://github.com/ag-ui-protocol/ag-ui.git',
      resolved_dependency_uri: 'git+https://github.com/ag-ui-protocol/ag-ui@refs/heads/main',
      resolved_git_commit: '0c0b88a3fe087a631decd6225efaf40e068e4449',
      version: '0.0.58',
    },
  },
  online_crate_user_agent: 'vestrace-p01-provenance/1.0',
  tooling: {
    'zod-to-json-schema': {
      version: '3.25.2',
    },
  },
  workspace_aliases: {
    a2a: { package: 'a2a-lf', version: '=0.3.0' },
    'a2a-client': { package: 'a2a-client-lf', version: '=0.2.1' },
    'a2a-server': { package: 'a2a-server-lf', version: '=0.4.1' },
  },
};
