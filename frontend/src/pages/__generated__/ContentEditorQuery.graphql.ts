/**
 * @generated SignedSource<<ae72de53f0b2bccceaa8540297731ac4>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ContentEditorQuery$variables = {
  id: string;
  type: string;
};
export type ContentEditorQuery$data = {
  readonly content: {
    readonly fields: any;
    readonly id: string;
    readonly status: string;
  } | null | undefined;
};
export type ContentEditorQuery = {
  response: ContentEditorQuery$data;
  variables: ContentEditorQuery$variables;
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
    "concreteType": "ContentItem",
    "kind": "LinkedField",
    "name": "content",
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
    "name": "ContentEditorQuery",
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
    "name": "ContentEditorQuery",
    "selections": (v2/*:: as any*/)
  },
  "params": {
    "cacheID": "97700eb5980d8a26f595d232e34ec2e7",
    "id": null,
    "metadata": {},
    "name": "ContentEditorQuery",
    "operationKind": "query",
    "text": "query ContentEditorQuery(\n  $type: String!\n  $id: ID!\n) {\n  content(type: $type, id: $id) {\n    id\n    status\n    fields\n  }\n}\n"
  }
};
})();

(node as any).hash = "6a47b02a7cc38ed540de0eb11e740991";

export default node;
