# Building the search index with Hugo

To do it natively you build a template and adding it as "export" format

`layouts/_default/baseof.json.json`:

```
[
  {{ block "response" .}}{{ end }}
]
```

`layouts/_default/list.json.json`:

```
{{ define "response" }}
[
  {{ range $index, $e := .Data.Pages }}
  {{ if $index }}, {{ end }}{{ .Render "item" }}
  {{ end }}
]
{{ end }}
```

`layouts/_default/item.json.json`:

```
{
  "title": "{{ .Title }}",
  "url" : "{{ .Permalink }}",
  "body" : "{{ .PlainWords }}"
}
```

`config.toml`:

```toml
# ...

[outputs]
    home = ["json"] # Index everything

# ...
```

Output file will be in `public/index.json`

See https://forestry.io/blog/build-a-json-api-with-hugo/ for more info.

# Credits

Tutorial created by [@Lusitaniae](https://github.com/Lusitaniae).
