package api

import "testing"

func TestNormalizeImagePublicIDAcceptsLegacyAndBase62IDs(t *testing.T) {
	tests := []struct {
		name  string
		value string
		want  bool
	}{
		{name: "legacy Base64 URL", value: "y0Qg5r8S-K2-", want: true},
		{name: "new Base62", value: "019AZaz0zz0z", want: true},
		{name: "invalid character", value: "019AZaz0zz0!", want: false},
		{name: "invalid length", value: "019AZaz0zz0", want: false},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			_, ok := normalizeImagePublicID(test.value)
			if ok != test.want {
				t.Fatalf("normalizeImagePublicID(%q) valid = %v, want %v", test.value, ok, test.want)
			}
		})
	}
}
