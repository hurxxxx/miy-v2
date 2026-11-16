"""Public address validation shared by installation and proxy rendering.

No DNS lookup or particular domain is required: IP/port and hostname profiles
have the same three browser purposes and per-installation frame boundary.
"""
import ipaddress
import json
import re
import urllib.parse

ADDRESS_KEYS = {'origin', 'consoleOrigin', 'devPortalOrigin', 'appFrameOrigin',
                'appFramePorts', 'allowHttp'}
BAD_PORTS = {1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
             103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
             512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
             995, 1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668,
             6669, 6679, 6697, 10080}


def canonical_origin(value, allow_http=False):
    parsed = urllib.parse.urlsplit(value)
    if (parsed.scheme not in ('http', 'https') or not parsed.hostname
            or parsed.username is not None or parsed.password is not None
            or parsed.path not in ('', '/') or parsed.query or parsed.fragment
            or any(c.isspace() or ord(c) < 32 or c in '\\$' for c in value)):
        raise ValueError('Use an explicit http:// or https:// origin without credentials or a path')
    hostname = parsed.hostname.encode('idna').decode('ascii').lower()
    try:
        address = ipaddress.ip_address(hostname)
    except ValueError:
        if not all(re.fullmatch(r'[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?', label)
                   for label in hostname.split('.')) or len(hostname) > 253:
            raise ValueError('Invalid server hostname') from None
        loopback = hostname == 'localhost'
    else:
        if '%' in hostname:
            raise ValueError('IPv6 scope identifiers are not public origins')
        loopback = address.is_loopback
        hostname = '[' + address.compressed + ']' if address.version == 6 else str(address)
    if parsed.scheme == 'http' and not loopback and not allow_http:
        raise ValueError('HTTP outside loopback requires MIY_ALLOW_HTTP=1 or --allow-http')
    if parsed.port == 0 or parsed.netloc.endswith(':'):
        raise ValueError('Server port must be between 1 and 65535')
    suffix = '' if parsed.port in (None, 443 if parsed.scheme == 'https' else 80) else ':' + str(parsed.port)
    return parsed.scheme + '://' + hostname + suffix


def load_addresses(path):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('Duplicate address setting: ' + key)
            result[key] = value
        return result
    values = json.loads(path.read_text(), object_pairs_hook=unique)
    if not isinstance(values, dict) or set(values) != ADDRESS_KEYS:
        raise ValueError('Address file must contain exactly: ' + ', '.join(sorted(ADDRESS_KEYS)))
    if type(values['allowHttp']) is not bool:
        raise ValueError('allowHttp must be a JSON boolean')
    for key in ADDRESS_KEYS - {'allowHttp', 'appFramePorts'}:
        if not isinstance(values[key], str) or not values[key]:
            raise ValueError(key + ' must be a nonempty string')
    if values['appFramePorts'] is not None and not isinstance(values['appFramePorts'], str):
        raise ValueError('appFramePorts must be a first-last string or null')
    return values


def validate_addresses(settings):
    """Return frame proxy/allocation values; reject collisions before writing state."""
    origins = [settings[key] for key in ('origin', 'consoleOrigin', 'devPortalOrigin')]
    allow_http = settings.get('allowHttp', False)
    if any(canonical_origin(value, allow_http) != value for value in origins):
        raise ValueError('Browser origins must be canonical')
    parsed = [urllib.parse.urlsplit(value) for value in origins]
    if len(set(origins)) != 3 or len({value.scheme for value in parsed}) != 1:
        raise ValueError('Portal, console and dev portal must be distinct origins using the same transport')
    template, ports = settings['appFrameOrigin'], settings.get('appFramePorts')
    if template.count('{key}') + template.count('{port}') != 1:
        raise ValueError('App frame origin needs one {key} or {port}')
    pool = []
    if '{key}' in template:
        prefix, suffix = template.split('{key}')
        label = prefix.partition('://')[2]
        if ports is not None or not suffix.startswith('.') or not re.fullmatch(r'[a-z0-9-]*', label):
            raise ValueError('Frame {key} must be in the first host label; appFramePorts must be null')
        sample = template.replace('{key}', '0' * 32)
        sources = template.partition('://')[0] + '://*.' + suffix[1:]
    else:
        if not template.endswith(':{port}') or not re.fullmatch(r'[0-9]+-[0-9]+', ports or ''):
            raise ValueError('Frame {port} needs a first-last port pool')
        first, last = map(int, ports.split('-'))
        if not 0 < first <= last <= 65535:
            raise ValueError('Invalid app frame port pool')
        pool = [number for number in range(first, last + 1) if number not in BAD_PORTS]
        if not pool:
            raise ValueError('App frame pool contains only browser-blocked ports')
        # Validate the literal first port too (e.g. :443 would normalize away).
        sample = template.replace('{port}', str(first))
        sources = template.replace('{port}', '*')
    if canonical_origin(sample, allow_http) != sample:
        raise ValueError('App frame template must form canonical origins')
    frame = urllib.parse.urlsplit(sample)
    if frame.scheme != parsed[0].scheme:
        raise ValueError('App frames must use the portal transport')
    for value in origins:
        if '{key}' in template:
            prefix, suffix = template.split('{key}')
            matches = re.fullmatch(re.escape(prefix) + r'[0-9a-f]{32}' + re.escape(suffix), value)
        else:
            candidate = urllib.parse.urlsplit(value)
            matches = candidate.hostname == frame.hostname and first <= (
                candidate.port or (443 if candidate.scheme == 'https' else 80)) <= last
        if matches:
            raise ValueError('App frame addresses overlap a platform origin')
    return {'sample': sample, 'pool': pool, 'sources': sources,
            'sites': ', '.join(template.replace('{port}', str(number)) for number in pool) if pool else sources}
