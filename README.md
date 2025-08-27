# ​ portals-nft-buyer

**Super-fast NFT Portals Auto-Buyer in Rust**

A lightweight Rust tool that sends around **140 requests per second** to purchase NFT portals in as little as **~40 ms after a trigger event**. Requires an `auth.txt` file with up to 80 HTTP authorization headers.

---

###  Features

- ⚡ **High throughput**: ~140 requests/second  
- ⏱ **Low latency**: Executes purchase operations within ~40 ms  
- 🔄 **Asynchronous flow**: Cancels pending threads and fires purchase logic instantly

---

###  Requirements

- Rust (latest stable version)
- `auth.txt` — one HTTP `Authorization` header per line
- API endpoint and request format details for the target NFT platform (source code contains clues)

---

###  Quick Start

```bash
git clone https://github.com/apchhui/portals-nft-buyer.git
cd portals-nft-buyer

# Create auth.txt with your tokens:
# e.g.:
# Authorization: Bearer <token1>
# Authorization: Bearer <token2>

cargo build --release
cargo run --release
```
