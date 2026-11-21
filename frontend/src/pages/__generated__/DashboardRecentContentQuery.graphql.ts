/**
 * @generated SignedSource<<cd09002a45ca8053af5221758146eed7>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type DashboardRecentContentQuery$variables = {
  type: string;
};
export type DashboardRecentContentQuery$data = {
  readonly contentList: ReadonlyArray<{
    readonly fields: any;
    readonly id: string;
    readonly status: string;
  }>;
};
export type DashboardRecentContentQuery = {
  response: DashboardRecentContentQuery$data;
  variables: DashboardRecentContentQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "type"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Literal",
        "name": "page",
        "value": 1
      },
      {
        "kind": "Literal",
        "name": "perPage",
        "value": 5
      },
      {
        "kind": "Variable",
        "name": "type",
        "variableName": "type"
      }
    ],
    "concreteType": "ContentItem",
    "kind": "LinkedField",
    "name": "contentList",
    "plural": true,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "id",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "status",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "fields",
        "storageKey": null
      }
    ],
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "DashboardRecentContentQuery",
    "selections": (v1/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "DashboardRecentContentQuery",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "f3f3ff81cdee872daaea8d5e0caa875c",
    "id": null,
    "metadata": {},
    "name": "DashboardRecentContentQuery",
    "operationKind": "query",
    "text": "query DashboardRecentContentQuery(\n  $type: String!\n) {\n  contentList(type: $type, page: 1, perPage: 5) {\n    id\n    status\n    fields\n  }\n}\n"
  }
};
})();

(node as any).hash = "5e0318bfe4515b019988d1fbb255e81e";

export default node;
