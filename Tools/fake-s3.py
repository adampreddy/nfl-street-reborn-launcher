"""A loopback stand-in for the R2 bucket, so a signing mistake is caught here
instead of against the real one.

    py -3 Tools\\fake-s3.py [--port N]

Prints "PORT <n>" on its first line of stdout the moment it is listening,
then serves forever. `Tools\\e2e-upload.py` launches this as a subprocess and
reads that line to learn which port to point `rebornctl` at.

What it genuinely does, not merely pretends to:

  - Every PUT, POST (?uploads or ?uploadId) and DELETE (?uploadId) must carry
    an `Authorization: AWS4-HMAC-SHA256 ...` header whose signature this
    server recomputes, from scratch, against the one access key and secret
    it knows, using the same canonical-request rules `reborn_core::publish::
    upload::sign` uses (see that module's doc comment). A wrong secret, a
    wrong access key id, or a body that does not hash to the declared
    `x-amz-content-sha256` is rejected with 403 and nothing is stored. GET
    and HEAD are never checked: a real bucket serves its public objects the
    same way, unauthenticated, through the CDN in front of it.
  - Multipart is real: CreateMultipartUpload hands back an upload id this
    server remembers, part PUTs are kept by part number under that id and
    answer with an ETag, CompleteMultipartUpload concatenates the parts in
    the order the request's XML lists them and forgets the upload id, and
    AbortMultipartUpload forgets it without assembling anything. An upload
    id that was never completed or aborted is what `/__uploads` reports as
    still open.

A handful of routes under `/__` are this script's own control surface, not
S3's. None of them touches the Authorization header:

    GET  /__order              {"order": [key, key, ...]}  arrival order,
                                one entry per object actually stored
                                (a part PUT is not an arrival; the object it
                                belongs to is, once completed)
    GET  /__uploads             {"open": [{"key":.., "upload_id":..}, ...]}
    POST /__control              body {"fail_parts": N} -- the next N part
                                PUTs (any object, any part) answer 500 and
                                store nothing, so a caller can force a
                                multipart upload to exhaust its retries and
                                prove the abort actually happens
    POST /__seed                body {"key":.., "body":..} stores a string
                                directly, no signature, not added to
                                /__order -- for planting a fixture (a
                                launcher release manifest) the CLI itself
                                never uploads
    POST /__reset                forgets every object, upload and order
"""
import argparse
import hashlib
import hmac
import http.server
import html
import json
import urllib.parse
import re
import sys
import threading
import uuid

# The one key this server ever accepts. `Tools/e2e-upload.py` stores these
# same two values into the vault with `rebornctl publish token
# --r2-access-from-file/--r2-secret-from-file` before pointing rebornctl at
# this server's port with `--endpoint`/`--bucket`.
ACCESS_KEY_ID = "FAKE00000000000000TESTKEY"
SECRET_ACCESS_KEY = "FAKEsecretaccesskeyFORtestingONLYnotReal1234567890"

# sha256 of the empty byte string. `CreateMultipartUpload` and
# `AbortMultipartUpload` sign this, matching upload.rs's own constant.
EMPTY_PAYLOAD_SHA256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
UNSIGNED_PAYLOAD = "UNSIGNED-PAYLOAD"


class Store:
    def __init__(self):
        self.lock = threading.Lock()
        self.objects: dict[str, bytes] = {}
        self.order: list[str] = []
        # upload_id -> {"key": str, "parts": {int: bytes}}
        self.multipart: dict[str, dict] = {}
        self.fail_parts_remaining = 0


STORE = Store()


# -- SigV4 verification, mirroring reborn_core::publish::upload::sign --------

def _hmac(key: bytes, data: str) -> bytes:
    return hmac.new(key, data.encode("utf-8"), hashlib.sha256).digest()


def _signing_key(secret: str, date_stamp: str, region: str) -> bytes:
    k_date = _hmac(("AWS4" + secret).encode("utf-8"), date_stamp)
    k_region = _hmac(k_date, region)
    k_service = _hmac(k_region, "s3")
    return _hmac(k_service, "aws4_request")


def _canonical_query(raw_query: str) -> str:
    """Sort the already-encoded `k=v` pairs as opaque strings, exactly as
    `canonical_query` in upload.rs sorts already-encoded `(String, String)`
    tuples. Never decoded and re-encoded: the wire bytes are the signed
    bytes, so re-deriving them by a different route risks disagreeing with
    what the client actually signed."""
    if not raw_query:
        return ""
    pairs = []
    for segment in raw_query.split("&"):
        if "=" in segment:
            k, v = segment.split("=", 1)
        else:
            k, v = segment, ""
        pairs.append((k, v))
    pairs.sort()
    return "&".join(f"{k}={v}" for k, v in pairs)


def _parse_authorization(value: str):
    """`AWS4-HMAC-SHA256 Credential=AKID/date/region/s3/aws4_request,
    SignedHeaders=a;b;c,Signature=hex` -> dict, or None if malformed."""
    prefix = "AWS4-HMAC-SHA256 "
    if not value or not value.startswith(prefix):
        return None
    fields = {}
    for part in value[len(prefix):].split(","):
        name, sep, val = part.strip().partition("=")
        if not sep:
            return None
        fields[name] = val
    credential = fields.get("Credential", "")
    cred_parts = credential.split("/")
    if len(cred_parts) != 5 or cred_parts[3] != "s3" or cred_parts[4] != "aws4_request":
        return None
    signed_headers = fields.get("SignedHeaders", "")
    signature = fields.get("Signature", "")
    if not signed_headers or not signature:
        return None
    return {
        "access_key_id": cred_parts[0],
        "date_stamp": cred_parts[1],
        "region": cred_parts[2],
        "signed_headers": signed_headers.split(";"),
        "signature": signature,
    }


def verify_signature(handler, method: str, path: str, raw_query: str, body: bytes) -> str | None:
    """`None` on success. A human-readable reason string on failure --
    never the secret, never the signature values, just what disagreed."""
    auth = handler.headers.get("Authorization")
    parsed = _parse_authorization(auth)
    if parsed is None:
        return "missing or malformed Authorization header"
    if parsed["access_key_id"] != ACCESS_KEY_ID:
        return "unknown access key id"

    amz_date = handler.headers.get("x-amz-date", "")
    payload_sha256 = handler.headers.get("x-amz-content-sha256", "")
    if not amz_date or not payload_sha256:
        return "missing x-amz-date or x-amz-content-sha256"

    if payload_sha256 != UNSIGNED_PAYLOAD:
        actual = hashlib.sha256(body).hexdigest()
        if actual != payload_sha256:
            return "x-amz-content-sha256 does not match the body actually sent"

    canonical_headers = "".join(
        f"{name}:{handler.headers.get(name, '').strip()}\n" for name in parsed["signed_headers"]
    )
    signed_headers_str = ";".join(parsed["signed_headers"])
    canonical_query = _canonical_query(raw_query)
    canonical_request = f"{method}\n{path}\n{canonical_query}\n{canonical_headers}\n{signed_headers_str}\n{payload_sha256}"
    hashed_canonical_request = hashlib.sha256(canonical_request.encode("utf-8")).hexdigest()

    credential_scope = f"{parsed['date_stamp']}/{parsed['region']}/s3/aws4_request"
    string_to_sign = f"AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{hashed_canonical_request}"

    signing_key = _signing_key(SECRET_ACCESS_KEY, parsed["date_stamp"], parsed["region"])
    expected = hmac.new(signing_key, string_to_sign.encode("utf-8"), hashlib.sha256).hexdigest()

    if not hmac.compare_digest(expected, parsed["signature"]):
        return "signature mismatch"
    return None


# -- the HTTP handler ---------------------------------------------------------

class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    # -- helpers --------------------------------------------------------

    def _split(self):
        path, _, query = self.path.partition("?")
        return path, query

    def _body(self) -> bytes:
        length = int(self.headers.get("Content-Length", 0) or 0)
        return self.rfile.read(length) if length else b""

    def _respond(self, status: int, body: bytes = b"", headers: dict | None = None):
        self.send_response(status)
        headers = dict(headers or {})
        # A caller sets Content-Length itself only for HEAD, where the real
        # object length is reported but no body is actually written.
        # Never send it twice: a duplicate header is a framing bug waiting
        # to confuse whichever HTTP client is more literal about it.
        headers.setdefault("Content-Length", str(len(body)))
        for k, v in headers.items():
            self.send_header(k, v)
        self.end_headers()
        if body:
            self.wfile.write(body)

    def _respond_json(self, status: int, obj):
        self._respond(status, json.dumps(obj).encode("utf-8"), {"Content-Type": "application/json"})

    def _key_from_path(self, path: str):
        """`/<bucket>/<key...>` -> key, or None when the path names no
        object at all (too short to hold a bucket and a key)."""
        trimmed = path.lstrip("/")
        if "/" not in trimmed:
            return None
        _bucket, key = trimmed.split("/", 1)
        return key or None

    def _refuse_signature(self, why: str):
        self._respond(403, f"SignatureDoesNotMatch: {why}".encode("utf-8"))

    @staticmethod
    def _parse_query(query: str) -> dict[str, str]:
        if not query:
            return {}
        pairs = {}
        for segment in query.split("&"):
            if not segment:
                continue
            name, sep, value = segment.partition("=")
            pairs[name] = value if sep else ""
        return pairs

    # -- S3-shaped routes -------------------------------------------------

    def do_GET(self):
        path, query = self._split()
        if path == "/__order":
            with STORE.lock:
                self._respond_json(200, {"order": list(STORE.order)})
            return
        if path == "/__uploads":
            with STORE.lock:
                open_uploads = [{"key": v["key"], "upload_id": k} for k, v in STORE.multipart.items()]
            self._respond_json(200, {"open": open_uploads})
            return
        # ListObjectsV2: GET /<bucket>/?list-type=2&prefix=&max-keys=&continuation-token=
        # A thousand keys a page, sorted, the way S3 and R2 answer. The
        # continuation token is the last key of the page, which is all a
        # token needs to be for a sorted listing.
        params = urllib.parse.parse_qs(query)
        if params.get("list-type") == ["2"]:
            prefix = params.get("prefix", [""])[0]
            max_keys = int(params.get("max-keys", ["1000"])[0])
            after = params.get("continuation-token", [None])[0]
            with STORE.lock:
                keys = sorted(k for k in STORE.objects if k.startswith(prefix) and (after is None or k > after))
                page = [(k, len(STORE.objects[k])) for k in keys[:max_keys]]
            truncated = len(keys) > max_keys
            body = ['<?xml version="1.0" encoding="UTF-8"?><ListBucketResult>']
            for k, n in page:
                body.append(f"<Contents><Key>{html.escape(k)}</Key><Size>{n}</Size></Contents>")
            body.append(f"<IsTruncated>{'true' if truncated else 'false'}</IsTruncated>")
            if truncated:
                body.append(f"<NextContinuationToken>{html.escape(page[-1][0])}</NextContinuationToken>")
            body.append("</ListBucketResult>")
            self._respond(200, "".join(body).encode("utf-8"))
            return
        key = self._key_from_path(path)
        if key is None:
            self._respond(400, b"no key in path")
            return
        with STORE.lock:
            body = STORE.objects.get(key)
        if body is None:
            self._respond(404, b"NoSuchKey")
            return
        self._respond(200, body)

    def do_HEAD(self):
        path, _query = self._split()
        key = self._key_from_path(path)
        if key is None:
            self._respond(400)
            return
        with STORE.lock:
            body = STORE.objects.get(key)
        if body is None:
            self._respond(404)
            return
        self._respond(200, headers={"Content-Length": str(len(body))})

    def do_PUT(self):
        path, query = self._split()
        body = self._body()
        params = self._parse_query(query)

        if "partNumber" in params and "uploadId" in params:
            self._put_part(path, query, body, params)
            return

        key = self._key_from_path(path)
        if key is None:
            self._respond(400, b"no key in path")
            return
        why = verify_signature(self, "PUT", path, query, body)
        if why is not None:
            self._refuse_signature(why)
            return
        with STORE.lock:
            STORE.objects[key] = body
            STORE.order.append(key)
        self._respond(200)

    def _put_part(self, path, query, body, params):
        upload_id = params["uploadId"]
        with STORE.lock:
            entry = STORE.multipart.get(upload_id)
        if entry is None:
            self._respond(404, b"NoSuchUpload")
            return
        why = verify_signature(self, "PUT", path, query, body)
        if why is not None:
            self._refuse_signature(why)
            return
        with STORE.lock:
            if STORE.fail_parts_remaining > 0:
                STORE.fail_parts_remaining -= 1
                self._respond(500, b"InternalError: told to fail this part for the test")
                return
            part_number = int(params["partNumber"])
            entry["parts"][part_number] = body
        etag = f'"{hashlib.md5(body).hexdigest()}"'
        self._respond(200, headers={"ETag": etag})

    def do_POST(self):
        path, query = self._split()
        body = self._body()

        if path == "/__control":
            data = json.loads(body) if body else {}
            with STORE.lock:
                STORE.fail_parts_remaining = int(data.get("fail_parts", 0))
            self._respond_json(200, {"ok": True})
            return
        if path == "/__seed":
            data = json.loads(body)
            with STORE.lock:
                STORE.objects[data["key"]] = data["body"].encode("utf-8")
            self._respond_json(200, {"ok": True})
            return
        if path == "/__reset":
            with STORE.lock:
                STORE.objects.clear()
                STORE.order.clear()
                STORE.multipart.clear()
                STORE.fail_parts_remaining = 0
            self._respond_json(200, {"ok": True})
            return

        params = self._parse_query(query)
        key = self._key_from_path(path)
        if key is None:
            self._respond(400, b"no key in path")
            return

        if "uploads" in params:
            why = verify_signature(self, "POST", path, query, body)
            if why is not None:
                self._refuse_signature(why)
                return
            upload_id = uuid.uuid4().hex
            with STORE.lock:
                STORE.multipart[upload_id] = {"key": key, "parts": {}}
            xml = (
                "<InitiateMultipartUploadResult>"
                f"<Key>{key}</Key><UploadId>{upload_id}</UploadId>"
                "</InitiateMultipartUploadResult>"
            )
            self._respond(200, xml.encode("utf-8"))
            return

        if "uploadId" in params:
            why = verify_signature(self, "POST", path, query, body)
            if why is not None:
                self._refuse_signature(why)
                return
            upload_id = params["uploadId"]
            with STORE.lock:
                entry = STORE.multipart.get(upload_id)
                if entry is None:
                    self._respond(404, b"NoSuchUpload")
                    return
                part_numbers = [int(n) for n in re.findall(r"<PartNumber>(\d+)</PartNumber>", body.decode("utf-8"))]
                missing = [n for n in part_numbers if n not in entry["parts"]]
                if missing:
                    self._respond(400, f"InvalidPart: {missing}".encode("utf-8"))
                    return
                assembled = b"".join(entry["parts"][n] for n in part_numbers)
                STORE.objects[key] = assembled
                STORE.order.append(key)
                del STORE.multipart[upload_id]
            self._respond(200, b"<CompleteMultipartUploadResult/>")
            return

        self._respond(400, b"unrecognized POST")

    def do_DELETE(self):
        path, query = self._split()
        params = self._parse_query(query)
        if "uploadId" not in params:
            self._respond(400, b"DELETE needs ?uploadId=")
            return
        why = verify_signature(self, "DELETE", path, query, b"")
        if why is not None:
            self._refuse_signature(why)
            return
        upload_id = params["uploadId"]
        with STORE.lock:
            existed = STORE.multipart.pop(upload_id, None) is not None
        self._respond(204 if existed else 404)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=0)
    args = parser.parse_args()

    server = http.server.ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"PORT {server.server_address[1]}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    sys.exit(main())
