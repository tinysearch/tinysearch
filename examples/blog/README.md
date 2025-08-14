# Blog Search Example

This example demonstrates how to use TinySearch for a technical blog with rich content and metadata.

## Configuration

The `tinysearch.toml` file configures the search to:

- **Index**: `title`, `content`, `excerpt`, and `tags` for comprehensive search coverage
- **Store as metadata**: `author`, `publish_date`, `category`, `reading_time`, `featured_image`
- **Use**: `permalink` as the link field for each blog post

## Sample Data

The `posts.json` file contains 10 technical blog posts covering:
- Programming languages (Rust, JavaScript)
- Web development (frameworks, CSS, performance)
- Backend development (databases, APIs)
- Cloud computing and architecture
- DevOps (Git workflows)
- Security best practices

Each post includes realistic content, author information, publication dates, and categorization.

## Usage

From this directory, run:

```bash
# Generate the search index
tinysearch -m storage -p ./output posts.json

# Search for programming topics
tinysearch -m search -S "rust programming" -N 3 ./output/storage

# Search for web development content
tinysearch -m search -S "javascript frameworks" -N 5 ./output/storage

# Find security-related posts
tinysearch -m search -S "security vulnerabilities" -N 2 ./output/storage
```

## For Blog Integration

Generate WASM files for your blog:

```bash
# Development version with demo
tinysearch -m wasm -p ./wasm_output posts.json

# Production version for deployment
tinysearch --release -m wasm -p ./wasm_output posts.json
```

## Search Examples

Readers can search for:
- **Technologies**: "rust", "javascript", "css", "database"
- **Topics**: "performance", "security", "architecture", "optimization"
- **Categories**: "frontend", "backend", "devops", "cloud"
- **Authors**: "Sarah Chen", "Alex Rodriguez"
- **Concepts**: "microservices", "responsive design", "git workflows"

Search results include author names, publication dates, reading times, and categories to help readers find exactly what they're looking for.