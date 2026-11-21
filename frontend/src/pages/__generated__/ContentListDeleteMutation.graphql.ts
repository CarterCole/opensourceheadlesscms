/**
 * @generated SignedSource<<c52e68758917c4f0d89f1dbe8e3e4f24>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ContentListDeleteMutation$variables = {
  id: string;
  type: string;
};
export type ContentListDeleteMutation$data = {
  readonly deleteContent: boolean;
};
export type ContentListDeleteMutation = {
  response: ContentListDeleteMutation$data;
  variables: ContentListDeleteMutation$variables;
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
    "alias": null,
    "args": [
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
    "kind": "ScalarField",
    "name": "deleteContent",
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
    "name": "ContentListDeleteMutation",
    "selections": (v2/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [
      (v1/*:: as any*/),
      (v0/*:: as any*/)
    ],
    "kind": "Operation",
    "name": "ContentListDeleteMutation",
    "selections": (v2/*:: as any*/)
  },
  "params": {
    "cacheID": "d40edd22fe7c70a143a28fc1bdd448bd",
    "id": null,
    "metadata": {},
    "name": "ContentListDeleteMutation",
    "operationKind": "mutation",
    "text": "mutation ContentListDeleteMutation(\n  $type: String!\n  $id: ID!\n) {\n  deleteContent(type: $type, id: $id)\n}\n"
  }
};
})();

(node as any).hash = "f95f9f13a157b9b740493b2cff562acc";

export default node;
