# ai-kda-community — source of truth for the Kadena Chainweb ecosystem

> Curated, machine-readable registry of canonical URLs, live networks, and deprecated resources. Check URLs, scan code for stale references, and look up endpoints — for humans, agents, and CI alike.

## Install

```
brew install ai-kda-community/tap/ai-kda-community
```

## Usage

```
ai-kda-community                 # overview with all sections
ai-kda-community good --json     # all valid resources as JSON
ai-kda-community bad  --json     # all deprecated resources as JSON
ai-kda-community check-url <url> # classify a URL
ai-kda-community scan <file>     # scan code for deprecated references
```

## ✓ Canonical URLs (28 — use these)

### Organizations

- <https://github.com/kda-community> — kda-community org
- <https://github.com/kda-community/chainweb-node> — chainweb-node
- <https://github.com/kda-community/kadena-docs-new> — kadena-docs source
- <https://github.com/kda-community/pact-5> — pact-5
- <https://github.com/kda-community/testnet06> — testnet06 bootstrap tx
- <https://kda-chain.org> — kda-chain.org

### Documentation

- <https://kda-chain.org/docs/> — community docs
- <https://pact-util-lib.readthedocs.io/en/beta_0.11/> — pact-util-lib

### Network endpoints

- <https://ca-1.chainweb-community.org> — mainnet bootstrap ca-1
- <https://fr-1.chainweb-community.org> — mainnet bootstrap fr-1
- <https://nl-1.chainweb-community.org> — mainnet bootstrap nl-1
- <https://pl-1.chainweb-community.org> — mainnet bootstrap pl-1
- <https://pl-2.chainweb-community.org> — mainnet bootstrap pl-2
- <https://testnet06-1.chainweb-community.org> — testnet06 bootstrap

### APIs

- <https://api.chainweb-community.org> — mainnet01 API
- <https://api.testnet.chainweb-community.org> — testnet06 API

### Explorers

- <https://denascan.com> — denascan
- <https://explorer.chainweb-community.org/mainnet> — explorer mainnet
- <https://explorer.chainweb-community.org/testnet> — explorer testnet
- <https://explorer.eckowallet.com/> — eckowallet

### Tools

- <https://github.com/kda-community/kda-tool> — kda-tool
- <https://ide.forkme.fun/> — ForkMe Pact IDE
- <https://snapshots.chainweb-community.org> — snapshots
- <https://tools.chainweb-community.org/> — community tools

### Faucets

- <https://tools.chainweb-community.org/faucet/existing> — mainnet faucet (existing account)
- <https://tools.chainweb-community.org/faucet/new> — testnet06 faucet (new account)

### Specs

- <https://api.chainweb-community.org/openapi/> — Chainweb OpenAPI spec

### Monitors

- <https://nodes.kadena.ws/> — network monitor

## Kadena commands

### Keys

- Generate a fresh keypair for signing
  ```
  kda-tool keygen -o key.yaml
  ```

### Faucets

- Request testnet06 coins via the community faucet
  ```
  open https://tools.chainweb-community.org/faucet/new
  ```

### Deploy

- Deploy a Pact module to mainnet01 chain 0
  ```
  kda-tool deploy --network mainnet01 --chain 0 --signer key.yaml --code module.pact
  ```
  _unverified: TODO: confirm exact flags against kda-tool --help_
- Deploy a Pact module to testnet06 chain 0
  ```
  kda-tool deploy --network testnet06 --chain 0 --signer key.yaml --code module.pact
  ```
  _unverified: TODO: confirm exact flags against kda-tool --help_
- Submit a signed transaction to a local node
  ```
  curl -sS -X POST http://localhost:1848/chainweb/0.0/mainnet01/chain/0/pact/api/v1/send -H 'Content-Type: application/json' -d @tx.json | jq
  ```

### Query

- List deployed modules on mainnet01 chain 0 via the community endpoint
  ```
  sot local mainnet01 0 '(list-modules)'
  ```
- Describe the coin module on mainnet01 chain 0 via the community endpoint
  ```
  sot local mainnet01 0 '(describe-module "coin")'
  ```
- Current cut from the community mainnet01 endpoint
  ```
  curl -sS 'https://api.chainweb-community.org/chainweb/0.0/mainnet01/cut' | jq
  ```
- Last 10 blocks on mainnet01 chain 0 via the community endpoint
  ```
  curl -sS 'https://api.chainweb-community.org/chainweb/0.0/mainnet01/chain/0/block?limit=10' | jq
  ```
- Last 10 headers on mainnet01 chain 0 via the community endpoint
  ```
  curl -sS 'https://api.chainweb-community.org/chainweb/0.0/mainnet01/chain/0/header?limit=10' | jq
  ```
- Fetch a mainnet01 chain 0 header by its hash via the community endpoint
  ```
  curl -sS -H 'Accept: application/json;blockheader-encoding=object' 'https://api.chainweb-community.org/chainweb/0.0/mainnet01/chain/0/header/FIe-VDNSOLAJtMtkQ75aTA0__r0iQadHwx73IYgWTE4' | jq
  ```
- Fetch a mainnet01 chain 0 payload by its hash via the community endpoint
  ```
  curl -sS 'https://api.chainweb-community.org/chainweb/0.0/mainnet01/chain/0/payload/2ixUhFWt9_1R-UemKElv7Of21nO3cscQqaTNnfZX5IA' | jq
  ```
- Fetch the outputs of a mainnet01 chain 0 payload via the community endpoint
  ```
  curl -sS 'https://api.chainweb-community.org/chainweb/0.0/mainnet01/chain/0/payload/2ixUhFWt9_1R-UemKElv7Of21nO3cscQqaTNnfZX5IA/outputs' | jq
  ```
- List deployed modules on a local node
  ```
  sot local -host http://localhost:1848 mainnet01 0 '(list-modules)'
  ```
- Describe the coin module on a local node
  ```
  sot local -host http://localhost:1848 mainnet01 0 '(describe-module "coin")'
  ```
- Current cut from a local node
  ```
  curl -sS http://localhost:1848/chainweb/0.0/mainnet01/cut | jq
  ```
- Last 10 blocks on mainnet01 chain 0 from a local node
  ```
  curl -sS 'http://localhost:1848/chainweb/0.0/mainnet01/chain/0/block?minheight=7315000&limit=10' | jq
  ```
- Last 10 headers on mainnet01 chain 0 from a local node
  ```
  curl -sS 'http://localhost:1848/chainweb/0.0/mainnet01/chain/0/header?limit=10' | jq
  ```
- Fetch a mainnet01 chain 0 header by its hash from a local node
  ```
  curl -sS -H 'Accept: application/json;blockheader-encoding=object' 'http://localhost:1848/chainweb/0.0/mainnet01/chain/0/header/FIe-VDNSOLAJtMtkQ75aTA0__r0iQadHwx73IYgWTE4' | jq
  ```
- Fetch a mainnet01 chain 0 payload by its hash from a local node
  ```
  curl -sS 'http://localhost:1848/chainweb/0.0/mainnet01/chain/0/payload/2ixUhFWt9_1R-UemKElv7Of21nO3cscQqaTNnfZX5IA' | jq
  ```
- Fetch the outputs of a mainnet01 chain 0 payload from a local node
  ```
  curl -sS 'http://localhost:1848/chainweb/0.0/mainnet01/chain/0/payload/2ixUhFWt9_1R-UemKElv7Of21nO3cscQqaTNnfZX5IA/outputs' | jq
  ```
- Stream new headers as they arrive from a local node
  ```
  curl -sS -N http://localhost:1848/chainweb/0.0/mainnet01/header/updates
  ```
- Poll a transaction result by request key
  ```
  curl -sS -X POST http://localhost:1848/chainweb/0.0/mainnet01/chain/0/pact/api/v1/poll -H 'Content-Type: application/json' -d '{"requestKeys":["2ixUhFWt9_1R-UemKElv7Of21nO3cscQqaTNnfZX5IA"]}' | jq
  ```

### Node

- Local node health check
  ```
  curl -sS http://localhost:1848/health-check
  ```

## Releases

- [chainweb-node](https://github.com/kda-community/chainweb-node/releases)
- [pact-5](https://github.com/kda-community/pact-5/releases)
- [kda-tool](https://github.com/kda-community/kda-tool/releases)
- [testnet06](https://github.com/kda-community/testnet06/releases)

## AI tooling

- **AI tooling** _(coming soon)_
- **Skills** _(coming soon)_
- **Prompts** _(coming soon)_
- **Harnesses** _(coming soon)_

## ✗ Deprecated (30 — do not use)

### archived_repo

- `https://github.com/kadena-io/chainweb-miner` → `https://github.com/kda-community/chainweb-mining-client`
- `https://github.com/kadena-io/chainweb-mining-client` → `https://github.com/kda-community/chainweb-mining-client`
- `https://github.com/kadena-io/chainweb-node` → `https://github.com/kda-community/chainweb-node`
- `https://github.com/kadena-io/developer-scripts` → `https://github.com/kda-community/developer-scripts`
- `https://github.com/kadena-io/kadena-docs` → `https://kda-chain.org/docs/`
- `https://github.com/kadena-io/kadenaswap` → `https://github.com/kda-community/kadenaswap`
- `https://github.com/kadena-io/kda-tool` → `https://github.com/kda-community/kda-tool`
- `https://github.com/kadena-io/marmalade` → `https://github.com/kda-community/marmalade`
- `https://github.com/kadena-io/pact-5` → `https://github.com/kda-community/pact-5`

### dead_docs

- `https://docs.kadena.io/` → `https://kda-chain.org/docs/`
- `https://kadena-io.github.io/kadena-docs/` → `https://kda-chain.org/docs/`
- `https://kadena-io.github.io/kadena-docs/Public-Chain-Docs/` → `https://kda-chain.org/docs/`

### dead_site

- `https://kadena.io/` → `https://kda-chain.org/`
- `https://kadena.io/developers` → `https://kda-chain.org/docs/`
- `https://kadena.io/docs` → `https://kda-chain.org/docs/`

### dead_tooling

- `https://github.com/kadena-io/chainweaver` → `https://ide.forkme.fun/`
- `https://github.com/kadena-io/kadena.js` → `https://github.com/kadena-community/kadena.js`
- `https://github.com/kadena-io/pact-lang-api` → `https://github.com/kadena-community/kadena.js`
- `https://github.com/kadena-io/pact/releases` → `https://github.com/kda-community/pact-5/releases`

### deprecated_api

- `https://ap1.chainweb.com` → `https://ca-1.chainweb-community.org`
- `https://ap2.chainweb.com` → `https://ca-1.chainweb-community.org`
- `https://api.chainweb.com` → `https://api.chainweb-community.org`
- `https://api.mainnet.kadena.network` → `https://api.chainweb-community.org`
- `https://api.testnet.chainweb.com` → `https://api.testnet.chainweb-community.org`
- `https://api.testnet.kadena.network` → `https://api.testnet.chainweb-community.org`
- `https://eu1.chainweb.com` → `https://nl-1.chainweb-community.org`
- `https://us-e1.chainweb.com` → `https://fr-1.chainweb-community.org`
- `https://us-w1.chainweb.com` → `https://nl-1.chainweb-community.org`

### deprecated_network

- `testnet04` → `testnet06`
- `testnet05` → `testnet06`

---

Generated from the registry — do not edit by hand. Source: <https://github.com/AI-KDA-Community/source_of_truth>
