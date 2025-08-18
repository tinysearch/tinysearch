# Building the search index with Hugo

To do it natively, you build a template and add it as an "export" format

`layouts/_default/list.json.json`:

```go
[
  {{ range $index , $e := .Site.RegularPages }}{{ if $index }}, {{end}}{{ dict "title" .Title "url" .Permalink "body" .Plain | jsonify }}{{end}}
]
```

`config.toml`:

```toml
# ...

[outputs]
    home = ["html","rss","json"] # Add json to the list

# ...
```

The output file will be in `public/index.json`

See https://forestry.io/blog/build-a-json-api-with-hugo/ for more info.

## Github action

If building and deploying your hugo website using Github actions, you can use [tinysearch-action](https://github.com/leonhfr/tinysearch-action#deploy-a-hugo-website-to-github-pages).

```yaml
- name: Build tinysearch
  uses: leonhfr/tinysearch-action@v1
  with:
    index: public/index.json
    output_dir: public/wasm
    output_types: |
      wasm
```

## Credits

Tutorial created by [@Lusitaniae](https://github.com/Lusitaniae); edited by [@lord-re](https://github.com/lord-re) and [@leonhfr](https://github.com/leonhfr).
