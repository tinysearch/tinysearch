# Building the search index with Hugo

To do it natively you build a template and adding it as "export" format

`layouts/_default/list.json.json`:

```
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

Output file will be in `public/index.json`

See https://forestry.io/blog/build-a-json-api-with-hugo/ for more info.

# Credits

Tutorial created by [@Lusitaniae](https://github.com/Lusitaniae) edited by [@lord-re](https://github.com/lord-re).
