# E-commerce Search Example

This example demonstrates how to use TinySearch for an e-commerce website with product data.

## Configuration

The `tinysearch.toml` file configures the search to:

- **Index**: `product_name`, `description`, `category`, and `tags` for full-text search
- **Store as metadata**: `price`, `image_url`, `brand`, `availability`, `rating`, `reviews_count`
- **Use**: `product_url` as the link field for each product

## Sample Data

The `products.json` file contains 10 sample products across different categories:
- Electronics (headphones, speakers, webcams)
- Gaming (keyboards)
- Accessories (charging pads, USB hubs)
- Wearables (smartwatches)
- Furniture (office chairs)
- Lighting (desk lamps)
- Storage (external drives)

## Usage

From this directory, run:

```bash
# Generate the search index
tinysearch -m storage -p ./output products.json

# Test searching for "wireless"
tinysearch -m search -S "wireless" -N 5 ./output/storage

# Test searching for "gaming"
tinysearch -m search -S "gaming" -N 3 ./output/storage
```

## For Web Integration

Generate WASM files for browser use:

```bash
# Development version with demo
tinysearch -m wasm -p ./wasm_output products.json

# Production version (no demo files)
tinysearch --release -m wasm -p ./wasm_output products.json
```

## Search Examples

Users can search for:
- Product names: "headphones", "keyboard", "webcam"
- Categories: "electronics", "gaming", "furniture"
- Features: "wireless", "waterproof", "rgb", "4k"
- Use cases: "office", "gaming", "travel", "outdoor"

The search results will include product metadata like price, brand, and availability for display in your e-commerce interface.