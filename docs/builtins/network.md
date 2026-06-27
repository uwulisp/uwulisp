---
title: Network
sidebar:
  order: 9
---

### `tcp-connect`
Opens a TCP connection to the given host and port, sends a message, and returns the full response as a string (reads until the peer closes the connection).

```
(tcp-connect host port message)  →  String
```

Requires exactly three arguments. Raises an error if the connection fails or times out (10 s).

**Example**
```
(tcp-connect "echo.example.com" 7 "hello")  →  "hello"
```

---

### `tcp-send`
Connects to the given host and port, sends a message, and returns the number of bytes written. Does **not** wait for a reply.

```
(tcp-send host port message)  →  Number
```

Requires exactly three arguments. Useful for fire-and-forget protocols such as syslog.

**Example**
```
(tcp-send "logger.internal" 514 "hello syslog")  →  12
```

---

### `http-get`
Performs a blocking HTTP GET request and returns a 3-element list of `(status headers body)`.

```
(http-get host port path)  →  (List String String String)
```

Requires exactly three arguments. Raises an error if the connection fails or times out (10 s).

**Example**
```
(http-get "example.com" 80 "/index.html")
  →  ("HTTP/1.1 200 OK" "Content-Type: text/html\r\n..." "<html>...</html>")
```

---

### `http-post`
Performs a blocking HTTP POST request with the given body and returns a 3-element list of `(status headers body)`.

```
(http-post host port path body)  →  (List String String String)
```

Requires exactly four arguments. The request is sent with `Content-Type: application/x-www-form-urlencoded`.

**Example**
```
(http-post "api.example.com" 80 "/submit" "key=value&foo=bar")
  →  ("HTTP/1.1 201 Created" "..." "")
```

---

### `http-put`
Performs a blocking HTTP PUT request with the given body and returns a 3-element list of `(status headers body)`.

```
(http-put host port path body)  →  (List String String String)
```

Requires exactly four arguments. The request is sent with `Content-Type: application/x-www-form-urlencoded`.

**Example**
```
(http-put "api.example.com" 80 "/data/42" "value=updated")
  →  ("HTTP/1.1 200 OK" "..." "")
```

---

### `http-patch`
Performs a blocking HTTP PATCH request with the given body and returns a 3-element list of `(status headers body)`.

```
(http-patch host port path body)  →  (List String String String)
```

Requires exactly four arguments. The request is sent with `Content-Type: application/x-www-form-urlencoded`.

**Example**
```
(http-patch "api.example.com" 80 "/data/42" "field=newvalue")
  →  ("HTTP/1.1 200 OK" "..." "")
```

---

### `http-delete`
Performs a blocking HTTP DELETE request and returns a 3-element list of `(status headers body)`.

```
(http-delete host port path)  →  (List String String String)
```

Requires exactly three arguments.

**Example**
```
(http-delete "api.example.com" 80 "/data/42")
  →  ("HTTP/1.1 204 No Content" "" "")
```

---

### `http-status`
Extracts the numeric HTTP status code from a response list returned by any `http-*` request.

```
(http-status response)  →  Number
```

Requires exactly one argument. Raises an error if the argument is not a valid response list.

**Example**
```
(http-status (http-get "example.com" 80 "/"))  →  200
```

---

### `http-body`
Extracts the body string from a response list returned by any `http-*` request.

```
(http-body response)  →  String
```

Requires exactly one argument. Raises an error if the argument is not a valid response list.

**Example**
```
(http-body (http-get "example.com" 80 "/"))  →  "<html>...</html>"
```

---

### `http-headers`
Extracts the raw headers string from a response list returned by any `http-*` request.

```
(http-headers response)  →  String
```

Requires exactly one argument. Raises an error if the argument is not a valid response list.

**Example**
```
(http-headers (http-get "example.com" 80 "/"))
  →  "Content-Type: text/html\r\nContent-Length: 1256\r\n..."
```