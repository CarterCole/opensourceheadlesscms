/**
 * @generated SignedSource<<ed6605b2f0de4745216044b096ea0891>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type MutationType = "CREATED" | "DELETED" | "TRANSITIONED" | "UPDATED" | "%future added value";
export type ContentEditorSubscription$variables = {
  id?: string | null | undefined;
  type: string;
};
export type ContentEditorSubscription$data = {
  readonly contentChanged: {
    readonly item: {
      readonly fields: any;
      readonly id: string;
      readonly status: string;
    } | null | undefined;
    readonly mutationType: MutationType;
  };
};
export type ContentEditorSubscription = {
  response: ContentEditorSubscription$data;
  variables: ContentEditorSubscription$variables;
};

const node: ConcreteRequest = (function(){
var v0 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "id"
},
v1 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "type"
},
v2 = [
  {
    "kind": "Variable",
    "name": "id",
    "variableName": "id"
  },
  {
    "kind": "Variable",
    "name": "type",
    "variableName": "type"
  }
],
v3 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "mutationType",
  "storageKey": null
},
v4 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v5 = {
  "alias": null,
  "args": null,
  "concreteType": "ContentItem",
  "kind": "LinkedField",
  "name": "item",
  "plural": false,
  "selections": [
    (v4/*:: as any*/),
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
};
return {
  "fragment": {
    "argumentDefinitions": [
      (v0/*:: as any*/),
      (v1/*:: as any*/)
    ],
    "kind": "Fragment",
    "metadata": null,
    "name": "ContentEditorSubscription",
    "selections": [
      {
        "alias": null,
        "args": (v2/*:: as any*/),
        "concreteType": "ContentEvent",
        "kind": "LinkedField",
        "name": "contentChanged",
        "plural": false,
        "selections": [
          (v3/*:: as any*/),
          (v5/*:: as any*/)
        ],
        "storageKey": null
      }
    ],
    "type": "Subscription",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [
      (v1/*:: as any*/),
      (v0/*:: as any*/)
    ],
    "kind": "Operation",
    "name": "ContentEditorSubscription",
    "selections": [
      {
        "alias": null,
        "args": (v2/*:: as any*/),
        "concreteType": "ContentEvent",
        "kind": "LinkedField",
        "name": "contentChanged",
        "plural": false,
        "selections": [
          (v3/*:: as any*/),
          (v5/*:: as any*/),
          (v4/*:: as any*/)
        ],
        "storageKey": null
      }
    ]
  },
  "params": {
    "cacheID": "da7c3b2bc8509e331b0d2f0079496f59",
    "id": null,
    "metadata": {},
    "name": "ContentEditorSubscription",
    "operationKind": "subscription",
    "text": "subscription ContentEditorSubscription(\n  $type: String!\n  $id: ID\n) {\n  contentChanged(type: $type, id: $id) {\n    mutationType\n    item {\n      id\n      status\n      fields\n    }\n    id\n  }\n}\n"
  }
};
})();

(node as any).hash = "b4a5b2e26e4100845c1bca7dc0f54590";

export default node;
