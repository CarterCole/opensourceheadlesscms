/**
 * @generated SignedSource<<dc662d43e13c6a84a8d508a0aaa9a6c8>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ContentListQuery$variables = {
  status?: string | null | undefined;
  type: string;
};
export type ContentListQuery$data = {
  readonly contentList: ReadonlyArray<{
    readonly fields: any;
    readonly id: string;
    readonly status: string;
    readonly updatedAt: any;
  }>;
};
export type ContentListQuery = {
  response: ContentListQuery$data;
  variables: ContentListQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "status"
},
v1 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "type"
},
v2 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "status",
        "variableName": "status"
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
        "name": "updatedAt",
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
    "argumentDefinitions": [
      (v0/*:: as any*/),
      (v1/*:: as any*/)
    ],
    "kind": "Fragment",
    "metadata": null,
    "name": "ContentListQuery",
    "selections": (v2/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [
      (v1/*:: as any*/),
      (v0/*:: as any*/)
    ],
    "kind": "Operation",
    "name": "ContentListQuery",
    "selections": (v2/*:: as any*/)
  },
  "params": {
    "cacheID": "4adff81888cebc43e6e9749cb2a21e15",
    "id": null,
    "metadata": {},
    "name": "ContentListQuery",
    "operationKind": "query",
    "text": "query ContentListQuery(\n  $type: String!\n  $status: String\n) {\n  contentList(type: $type, status: $status) {\n    id\n    status\n    updatedAt\n    fields\n  }\n}\n"
  }
};
})();

(node as any).hash = "ac89eb6a993cfa3dd9bdfbcdc2ecf42b";

export default node;
