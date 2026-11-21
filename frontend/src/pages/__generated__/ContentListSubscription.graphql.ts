/**
 * @generated SignedSource<<3382c7f7d4a1cc9de66efed6117574bf>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type MutationType = "CREATED" | "DELETED" | "TRANSITIONED" | "UPDATED" | "%future added value";
export type ContentListSubscription$variables = {
  type: string;
};
export type ContentListSubscription$data = {
  readonly contentChanged: {
    readonly id: string;
    readonly item: {
      readonly fields: any;
      readonly id: string;
      readonly status: string;
      readonly updatedAt: any;
    } | null | undefined;
    readonly mutationType: MutationType;
  };
};
export type ContentListSubscription = {
  response: ContentListSubscription$data;
  variables: ContentListSubscription$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "type"
  }
],
v1 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v2 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "type",
        "variableName": "type"
      }
    ],
    "concreteType": "ContentEvent",
    "kind": "LinkedField",
    "name": "contentChanged",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "mutationType",
        "storageKey": null
      },
      (v1/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "concreteType": "ContentItem",
        "kind": "LinkedField",
        "name": "item",
        "plural": false,
        "selections": [
          (v1/*:: as any*/),
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
    ],
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "ContentListSubscription",
    "selections": (v2/*:: as any*/),
    "type": "Subscription",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ContentListSubscription",
    "selections": (v2/*:: as any*/)
  },
  "params": {
    "cacheID": "02a248d327f7a49498662dafbd879f8b",
    "id": null,
    "metadata": {},
    "name": "ContentListSubscription",
    "operationKind": "subscription",
    "text": "subscription ContentListSubscription(\n  $type: String!\n) {\n  contentChanged(type: $type) {\n    mutationType\n    id\n    item {\n      id\n      status\n      updatedAt\n      fields\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "dc8ec93fa82cb660088d5e6f89840568";

export default node;
