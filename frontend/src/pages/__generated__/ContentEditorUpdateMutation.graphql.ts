/**
 * @generated SignedSource<<1b1dd18ba24a1699f1f19e45303ba45d>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ContentEditorUpdateMutation$variables = {
  id: string;
  input: any;
  type: string;
};
export type ContentEditorUpdateMutation$data = {
  readonly updateContent: {
    readonly fields: any;
    readonly id: string;
    readonly status: string;
  };
};
export type ContentEditorUpdateMutation = {
  response: ContentEditorUpdateMutation$data;
  variables: ContentEditorUpdateMutation$variables;
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
  "name": "input"
},
v2 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "type"
},
v3 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "id",
        "variableName": "id"
      },
      {
        "kind": "Variable",
        "name": "input",
        "variableName": "input"
      },
      {
        "kind": "Variable",
        "name": "type",
        "variableName": "type"
      }
    ],
    "concreteType": "ContentItem",
    "kind": "LinkedField",
    "name": "updateContent",
    "plural": false,
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
    "argumentDefinitions": [
      (v0/*:: as any*/),
      (v1/*:: as any*/),
      (v2/*:: as any*/)
    ],
    "kind": "Fragment",
    "metadata": null,
    "name": "ContentEditorUpdateMutation",
    "selections": (v3/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [
      (v2/*:: as any*/),
      (v0/*:: as any*/),
      (v1/*:: as any*/)
    ],
    "kind": "Operation",
    "name": "ContentEditorUpdateMutation",
    "selections": (v3/*:: as any*/)
  },
  "params": {
    "cacheID": "554b9ffa6acc57a158b912987d9c2923",
    "id": null,
    "metadata": {},
    "name": "ContentEditorUpdateMutation",
    "operationKind": "mutation",
    "text": "mutation ContentEditorUpdateMutation(\n  $type: String!\n  $id: ID!\n  $input: JSON!\n) {\n  updateContent(type: $type, id: $id, input: $input) {\n    id\n    status\n    fields\n  }\n}\n"
  }
};
})();

(node as any).hash = "ab5920767d1a032e0ef2d99cb7c1a8a0";

export default node;
