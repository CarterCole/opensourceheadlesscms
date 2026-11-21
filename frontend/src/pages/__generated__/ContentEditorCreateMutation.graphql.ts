/**
 * @generated SignedSource<<7ffb272424fcc75b6b1af91e84203e60>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ContentEditorCreateMutation$variables = {
  input: any;
  type: string;
};
export type ContentEditorCreateMutation$data = {
  readonly createContent: {
    readonly fields: any;
    readonly id: string;
    readonly status: string;
  };
};
export type ContentEditorCreateMutation = {
  response: ContentEditorCreateMutation$data;
  variables: ContentEditorCreateMutation$variables;
};

const node: ConcreteRequest = (function(){
var v0 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "input"
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
    "name": "createContent",
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
      (v1/*:: as any*/)
    ],
    "kind": "Fragment",
    "metadata": null,
    "name": "ContentEditorCreateMutation",
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
    "name": "ContentEditorCreateMutation",
    "selections": (v2/*:: as any*/)
  },
  "params": {
    "cacheID": "eb6ee2845dc289e6eae6fd388a9de609",
    "id": null,
    "metadata": {},
    "name": "ContentEditorCreateMutation",
    "operationKind": "mutation",
    "text": "mutation ContentEditorCreateMutation(\n  $type: String!\n  $input: JSON!\n) {\n  createContent(type: $type, input: $input) {\n    id\n    status\n    fields\n  }\n}\n"
  }
};
})();

(node as any).hash = "ad94856add46fd419143e64db63c9c17";

export default node;
