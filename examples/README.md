# TinySearch Examples

This directory contains comprehensive examples demonstrating TinySearch usage for different types of websites and applications.

## Available Examples

### 🛒 [E-commerce](./ecommerce/)
Product catalog search with pricing, availability, and category metadata.
- **Data**: 10 sample products across multiple categories
- **Fields**: Product names, descriptions, categories, tags, pricing, availability
- **Use Case**: Online stores, marketplaces, product directories

### 📝 [Blog](./blog/)
Technical blog search with author information and content categorization.
- **Data**: 10 technical articles on programming and web development
- **Fields**: Titles, content, excerpts, tags, authors, publication dates
- **Use Case**: Personal blogs, corporate blogs, technical documentation

### 📚 [Documentation](./documentation/)
Comprehensive documentation search with version control and organization.
- **Data**: 11 documentation pages covering features, guides, and references
- **Fields**: Titles, content, sections, keywords, versions, contributors
- **Use Case**: Software documentation, API docs, knowledge bases

## Quick Start

Each example directory contains:
- `tinysearch.toml` - Configuration file defining the search schema
- `*.json` - Sample data file with realistic content
- `README.md` - Detailed instructions and usage examples

To try any example:

```bash
cd examples/[example-name]
tinysearch -m storage -p ./output [data-file].json
tinysearch -m search -S "your query" -N 5 ./output/storage
```

## Schema Customization

Each example demonstrates different schema configurations:

| Example | Indexed Fields | Metadata Fields | URL Field |
|---------|---------------|----------------|-----------|
| E-commerce | product_name, description, category, tags | price, brand, availability, rating | product_url |
| Blog | title, content, excerpt, tags | author, publish_date, category, reading_time | permalink |
| Documentation | title, content, section, keywords | version, last_updated, contributor, difficulty | doc_url |

## Integration Examples

### Static Site Generators

All examples work with popular static site generators:

- **Jekyll**: Use liquid templates to generate JSON from post frontmatter
- **Hugo**: Leverage JSON output formats for automatic index generation  
- **Gatsby**: Generate indices programmatically during build process
- **Next.js**: Create JSON during static generation phase

### Web Integration

Generate WASM files for browser deployment:

```bash
# Development with demo interface
tinysearch -m wasm -p ./wasm_output [data-file].json

# Production deployment
tinysearch --release -m wasm -p ./wasm_output [data-file].json
```

## Performance Characteristics

| Example | Index Size | Search Time | Memory Usage |
|---------|------------|-------------|--------------|
| E-commerce (10 products) | ~8KB | <1ms | ~2MB |
| Blog (10 posts) | ~12KB | <1ms | ~3MB |
| Documentation (11 pages) | ~15KB | <1ms | ~4MB |

*Measurements approximate, actual results vary by content and browser*

## Best Practices

1. **Field Selection**: Only index fields users will search; use metadata for display-only data
2. **Content Optimization**: Remove HTML tags and minimize unnecessary text
3. **Keyword Strategy**: Include relevant keywords and synonyms in indexed fields
4. **File Size**: Consider splitting large datasets into multiple indices
5. **WASM Optimization**: Use `--optimize` flag for production deployments

## Contributing Examples

To contribute new examples:
1. Create a new directory with a descriptive name
2. Include `tinysearch.toml`, sample JSON data, and README.md
3. Ensure data is realistic and demonstrates clear use cases
4. Test all commands mentioned in the README
5. Submit a pull request with your example

Examples should showcase different industries, content types, or technical approaches to help users understand TinySearch's flexibility.