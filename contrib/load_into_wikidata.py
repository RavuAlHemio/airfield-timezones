#!/usr/bin/env python3
import argparse
import json
import re
from typing import NamedTuple, Optional
import requests


PROPERTY_LOCATED_IN_TIME_ZONE = "P421"
USER_AGENT = "AirfieldTimezones (https://github.com/RavuAlHemio/airfield-timezones)"
ENTITY_RE = re.compile("^Q(?P<numeric>[0-9]+)$")


class Airport(NamedTuple):
    entity: str
    icao: str
    name: str
    timezone_entity: Optional[str]
    timezone_name: Optional[str]


def get_oauth_token(oauth_json_file: str) -> str:
    with open(oauth_json_file, "r", encoding="utf-8") as f:
        j = json.load(f)
    return j["access_token"]


def get_iana_timezone_to_wikidata(
    iana_timezone_query: str,
    sparql_endpoint: str,
    entity_prefix: str,
) -> dict[str, str]:
    if sparql_endpoint.startswith("file://"):
        path = sparql_endpoint.removeprefix("file://") + "wikidata_iana_timezone.json"
        with open(path, "r", encoding="utf-8") as f:
            json_doc = json.load(f)
    else:
        timezones_response = requests.post(
            sparql_endpoint,
            data={
                "query": iana_timezone_query,
                "format": "json",
            },
            headers={
                "User-Agent": USER_AGENT,
            },
        )
        timezones_response.raise_for_status()
        json_doc = timezones_response.json()

    iana_timezone_to_wikidata: dict[str, str] = {}
    for binding in json_doc["results"]["bindings"]:
        wikidata_item = binding["timezone"]["value"].removeprefix(entity_prefix)
        iana_timezone = binding["zoneName"]["value"]
        iana_timezone_to_wikidata[iana_timezone] = wikidata_item
    return iana_timezone_to_wikidata


def get_icao_to_airport(
    airport_icao_query: str,
    sparql_endpoint: str,
    entity_prefix: str,
) -> dict[str, Airport]:
    if sparql_endpoint.startswith("file://"):
        path = sparql_endpoint.removeprefix("file://") + "wikidata_airport_icao.json"
        with open(path, "r", encoding="utf-8") as f:
            json_doc = json.load(f)
    else:
        airports_response = requests.post(
            sparql_endpoint,
            data={
                "query": airport_icao_query,
                "format": "json",
            },
            headers={
                "User-Agent": USER_AGENT,
            },
        )
        airports_response.raise_for_status()
        json_doc = airports_response.json()

    icao_to_airport: dict[str, Airport] = {}
    for binding in json_doc["results"]["bindings"]:
        icao_code = binding["icaoCode"]["value"]
        if icao_code in icao_to_airport:
            continue
        entity = binding["airport"]["value"].removeprefix(entity_prefix)
        name = binding["airportLabel"]["value"]
        timezone_entity = binding.get("timezone", {}).get("value", None)
        if timezone_entity is not None:
            timezone_entity = timezone_entity.removeprefix(entity_prefix)
        timezone_name = binding.get("zoneName", {}).get("value", None)
        icao_to_airport[icao_code] = Airport(
            entity=entity,
            icao=icao_code,
            name=name,
            timezone_entity=timezone_entity,
            timezone_name=timezone_name,
        )
    return icao_to_airport


def get_icao_to_timezone(filename: str) -> dict[str, str]:
    icao_to_timezone: dict[str, str] = {}
    with open(filename, "r", encoding="utf-8") as f:
        for raw_ln in f:
            ln = raw_ln.rstrip("\r\n")
            pieces = ln.split(" ", 1)
            if len(pieces) != 2:
                continue
            if pieces[1] == "?":
                continue
            icao_to_timezone[pieces[0]] = pieces[1]
    return icao_to_timezone


def main():
    parser = argparse.ArgumentParser(
        description="Loads airport IANA timezone information into a Wikibase installation like Wikidata.",
    )
    parser.add_argument(
        "--api-endpoint",
        dest="api_endpoint", default="https://www.wikidata.org/w/api.php",
        help="Wikibase API endpoint to contact."
    )
    parser.add_argument(
        "--entity-prefix",
        dest="entity_prefix", default="http://www.wikidata.org/entity/",
        help="Entity ID prefix to remove."
    )
    parser.add_argument(
        "--sparql-endpoint",
        dest="sparql_endpoint", default="https://query.wikidata.org/sparql",
        help="Wikibase SPARQL endpoint to contact."
    )
    parser.add_argument(
        "--iana-timezone-query",
        dest="iana_timezone_query", default="wikidata_iana_timezone.sparql",
        help="File containing query to obtain the items that define IANA timezones."
    )
    parser.add_argument(
        "--airport-icao-query",
        dest="airport_icao_query", default="wikidata_airport_icao.sparql",
        help="File containing query to obtain the items that define airports."
    )
    parser.add_argument(
        "--oauth-config",
        dest="oauth_config", metavar="OAUTH.JSON", default="wikidata_oauth.json",
        help="File containing Wikimedia OAuth credentials."
    )
    parser.add_argument(
        "--airport",
        dest="airport", metavar="ICAO", default=None,
        help="ICAO code of airport to import."
    )
    parser.add_argument(
        "--yes-all-airports",
        dest="yes_all_airports", action="store_true",
        help="Whether to actually import all airports."
    )
    parser.add_argument(
        dest="icao_to_timezone", metavar="ICAO_TO_TIMEZONE",
        help="File containing mappings of ICAO airport codes to IANA timezones.",
    )
    args = parser.parse_args()
    if args.airport is None and not args.yes_all_airports:
        raise ValueError("either '--airport ICAO' or '--yes-all-airports' must be passed")
    if args.airport is not None and args.yes_all_airports:
        raise ValueError("'--airport ICAO' and '--yes-all-airports' may not be passed simultaneously")

    with open(args.iana_timezone_query, "r", encoding="utf-8") as f:
        iana_timezone_query = f.read()
    with open(args.airport_icao_query, "r", encoding="utf-8") as f:
        airport_icao_query = f.read()
    oauth_token = get_oauth_token(args.oauth_config)

    iana_timezone_to_wikidata = get_iana_timezone_to_wikidata(
        iana_timezone_query,
        args.sparql_endpoint,
        args.entity_prefix,
    )
    icao_to_airport = get_icao_to_airport(
        airport_icao_query,
        args.sparql_endpoint,
        args.entity_prefix,
    )
    icao_to_timezone = get_icao_to_timezone(args.icao_to_timezone)

    bearer_token = f"Bearer {oauth_token}"
    for _icao, airport in sorted(icao_to_airport.items()):
        if airport.icao != args.airport and not args.yes_all_airports:
            continue

        if airport.timezone_entity is not None:
            # we already know the timezone
            continue
        timezone = icao_to_timezone.get(airport.icao, None)
        if timezone is None:
            # we don't know the timezone
            continue
        timezone_entity = iana_timezone_to_wikidata.get(timezone, None)
        if timezone_entity is None:
            raise ValueError(f"IANA timezone {timezone!r} unknown to Wikibase")
        timezone_entity_match = ENTITY_RE.search(timezone_entity)
        if timezone_entity_match is None:
            raise ValueError(f"timezone entity {timezone_entity!r} is invalid")
        timezone_entity_id = int(timezone_entity_match.group("numeric"))

        csrf_token_response = requests.get(
            args.api_endpoint,
            params={
                "format": "json",
                "action": "query",
                "meta": "tokens",
                "type": "csrf",
            },
            headers={
                "User-Agent": USER_AGENT,
                "Authorization": bearer_token,
            },
        )
        csrf_token_response.raise_for_status()
        csrf_token = csrf_token_response.json()["query"]["tokens"]["csrftoken"]

        claim_response = requests.post(
            args.api_endpoint,
            params={
                "format": "json",
                "action": "wbcreateclaim",
                "entity": airport.entity,
                "property": PROPERTY_LOCATED_IN_TIME_ZONE,
                "snaktype": "value",
                "value": json.dumps({
                    "entity-type": "item",
                    "numeric-id": timezone_entity_id,
                }),
            },
            data={
                "token": csrf_token,
            },
            headers={
                "User-Agent": USER_AGENT,
                "Authorization": bearer_token,
            },
        )
        print(claim_response.status_code)
        print(claim_response.text)


if __name__ == "__main__":
    main()
